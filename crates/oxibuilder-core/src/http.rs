use crate::build::build_site;
use crate::build_writer::write_build_output;
use crate::error::ApiError;
use crate::extension::{
    CliArgSpec, CliCommandManifest, CliCommandSpec, CliSubcommandSpec, Extension, Lang,
};
use crate::search::SearchHit;
use crate::setup;
use crate::state::AppState;
use crate::theme;

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::http::{Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[derive(RustEmbed)]
#[folder = "embedded-spa"]
struct Assets;

#[derive(serde::Serialize)]
struct DataEnvelope<T: serde::Serialize> {
    data: T,
}

// Manifest + lobby-config types live in `crate::manifest` so the SSG build emits the identical
// shape the live handler serves (single source of truth for `fetchManifest()`).
use crate::manifest::{Manifest, ManifestLocalized};

/// Build the Oxibuilder HTTP application router.
///
/// If `console_routes` is provided, its routes are nested under
/// `/api/console` alongside the built-in API routes.
pub fn build_app(state: AppState) -> Router {
    let mut api = Router::new()
        .route("/lobby/manifest", get(lobby_manifest))
        .route("/lobby/config", get(lobby_config_list))
        .route(
            "/lobby/config/{ext_id}",
            axum::routing::put(lobby_config_update),
        )
        .route("/search", get(search_handler))
        .route("/docs", get(docs_ui))
        .route("/docs/openapi.json", get(docs_spec))
        .route("/extensions", get(extensions_list))
        .route(
            "/extensions/{id}/enable",
            axum::routing::post(extension_enable),
        )
        .route(
            "/extensions/{id}/disable",
            axum::routing::post(extension_disable),
        )
        .route("/extensions/{id}", axum::routing::delete(extension_purge))
        .route(
            "/extensions/install",
            axum::routing::post(extension_install),
        )
        .route("/extensions/registry", axum::routing::get(registry_list))
        .route("/backup/snapshot", axum::routing::post(backup_snapshot))
        .route("/cli/commands", get(cli_commands_handler))
        .route(
            "/cli/exec/{ext_id}/{sub_command}",
            axum::routing::post(cli_exec_handler),
        )
        .route("/themes", get(theme_catalog))
        .route("/build", axum::routing::post(build_handler))
        .route("/cache/refresh", axum::routing::post(cache_refresh_handler));

    // Setup API (loopback-only, unauthenticated, doc/13)
    api = setup::setup_routes(api);

    let api = api
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            setup::setup_gate,
        ))
        .fallback(api_fallback)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            extension_gate,
        ));

    let limiter = crate::rate_limit::RateLimiter::new(120); // IP당 120/min
    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/console", api)
        .fallback(static_handler)
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            limiter,
            crate::rate_limit::rate_limit_middleware,
        ))
        .layer(TraceLayer::new_for_http())
}

async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn docs_spec(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(crate::openapi::openapi_spec(
        &state.config.site.base_url,
        &state.registry,
    ))
}

async fn build_handler(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let config = &state.config;
    let out_dir = config.server.data_dir.join("out");
    let media_dir = config.server.data_dir.join("media");
    let inactive = crate::manifest::inactive_extension_ids(&state.db).await;
    let output = build_site(&state.db, &state.builders, &inactive)
        .map_err(|e| ApiError::internal(anyhow::anyhow!("{}", e)))?;

    let theme_id = crate::theme::active_theme_id(&state.db).await;
    let layout_id = crate::theme::active_layout_id(&state.db, &config.lobby.layout).await;
    let mut inputs =
        crate::builder::BuildInputs::new(&config.site.base_url, theme_id, layout_id, "oxibuilder");
    inputs.mounts = config
        .mounts
        .iter()
        .map(crate::builder::MountCopy::from_config)
        .collect();
    write_build_output(&output, &out_dir, &media_dir, &inputs)
        .map_err(|e| ApiError::internal(anyhow::anyhow!("{}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "pages": output.pages.len(),
        "extensions": output.extensions_data.len(),
        "out_dir": out_dir.to_string_lossy(),
    })))
}

async fn cache_refresh_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut scheduler = crate::scheduler::Scheduler::new();
    for ext in state.registry.iter() {
        for job in ext.background_jobs() {
            scheduler.register(job);
        }
    }

    let job_count = scheduler.jobs().len();
    scheduler.run_all_once(&state).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "jobs_run": job_count,
    })))
}

async fn docs_ui() -> axum::response::Response {
    let html = crate::openapi::swagger_ui_html("/api/console/docs/openapi.json");
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}
/// 동적 라우트 폴백. WASM(런타임 적재) 확장의 HTTP 요청을 디스패치한다.
/// `/api/console/{ext_id}/**` 경로에서 ext_id가 WASM 확장이면 route_dispatcher 로 위임.
/// 그 외에는 일반 404.
async fn api_fallback(State(state): State<AppState>, request: Request) -> Response {
    let path = request.uri().path().to_string();
    let method = request.method().to_string();
    if let Some(ext_id) = extension_id_from_path(&path)
        && let Some(ext) = state.registry.find(&ext_id)
        && let Some(dispatcher) = ext.route_dispatcher()
    {
        // 확장 prefix 이후 경로 추출 ("/wasm-demo/info" → "/info").
        let prefix_len = 1 + ext_id.len(); // '/' + ext_id
        let sub_path = if path.len() > prefix_len {
            &path[prefix_len..]
        } else {
            "/"
        };
        let body = axum::body::to_bytes(request.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default()
            .to_vec();
        let resp = dispatcher.dispatch(&method, sub_path, body, &state).await;
        return (
            StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK),
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            resp.body,
        )
            .into_response();
    }
    ApiError::new(StatusCode::NOT_FOUND, "not_found", "resource not found").into_response()
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
    lang: Option<String>,
    limit: Option<i64>,
}

async fn search_handler(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<DataEnvelope<Vec<SearchHit>>>, ApiError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let hits = crate::search::search(&state.db, &q.q, q.lang.as_deref(), limit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope { data: hits }))
}

async fn lobby_manifest(State(state): State<AppState>) -> Json<DataEnvelope<Manifest>> {
    let extensions = state.registry.iter();
    let manifest = crate::manifest::assemble(
        &state.db,
        &state.config,
        &state.effective_site_name().await,
        &state.effective_base_url().await,
        &extensions,
    )
    .await;
    Json(DataEnvelope { data: manifest })
}

async fn lobby_config_list(
    State(state): State<AppState>,
) -> Json<DataEnvelope<Vec<LobbyConfigEntry>>> {
    let mut entries = Vec::new();
    for (idx, e) in state.registry.iter().into_iter().enumerate() {
        if !state.registry.is_active(e.id()).await {
            continue;
        }
        let info =
            crate::manifest::lobby_config_for(&state.db, &state.config, e.id(), idx as i64).await;
        entries.push(LobbyConfigEntry {
            extension_id: e.id().to_string(),
            enabled: info.enabled,
            display_mode: info.display_mode,
            display_order: info.display_order,
            style_params: info.style_params,
        });
    }
    Json(DataEnvelope { data: entries })
}

#[derive(serde::Serialize)]
struct LobbyConfigEntry {
    extension_id: String,
    enabled: bool,
    display_mode: String,
    display_order: i64,
    style_params: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct LobbyConfigUpdate {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    display_mode: Option<String>,
    #[serde(default)]
    display_order: Option<i64>,
    #[serde(default)]
    style_params: Option<serde_json::Value>,
}

async fn lobby_config_update(
    State(state): State<AppState>,
    Path(ext_id): Path<String>,
    Json(input): Json<LobbyConfigUpdate>,
) -> Result<Json<DataEnvelope<LobbyConfigEntry>>, ApiError> {
    if state.registry.find(&ext_id).is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "extension not registered",
        ));
    }
    if let Some(ref mode) = input.display_mode
        && !matches!(mode.as_str(), "canvas" | "grid" | "list")
    {
        return Err(ApiError::validation(
            "display_mode",
            "display_mode must be canvas|grid|list",
        ));
    }
    let info = crate::manifest::lobby_config_for(&state.db, &state.config, &ext_id, 0).await;
    let enabled = input.enabled.unwrap_or(info.enabled);
    let mode = input.display_mode.unwrap_or(info.display_mode);
    let order = input.display_order.unwrap_or(info.display_order);
    let params = input.style_params.unwrap_or(info.style_params);
    let params_str = serde_json::to_string(&params).unwrap_or_else(|_| "{}".into());

    sqlx::query(
        "INSERT INTO lobby_config (extension_id, enabled, display_mode, display_order, style_params)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (extension_id) DO UPDATE SET
            enabled = ?2, display_mode = ?3, display_order = ?4, style_params = ?5",
    )
    .bind(&ext_id)
    .bind(enabled)
    .bind(&mode)
    .bind(order)
    .bind(&params_str)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::internal(anyhow::anyhow!(e)))?;

    Ok(Json(DataEnvelope {
        data: LobbyConfigEntry {
            extension_id: ext_id,
            enabled,
            display_mode: mode,
            display_order: order,
            style_params: params,
        },
    }))
}

// ─── backup (doc/05 §5.4) ───

/// `VACUUM INTO` 포인트-인-타임 스냅샷. admin 스코프 필요.
/// `data_dir/backups/oxibuilder-<epoch>.db`에 일관된 DB 복사본을 생성한다.
async fn backup_snapshot(
    State(state): State<AppState>,
) -> Result<Json<DataEnvelope<serde_json::Value>>, ApiError> {
    let backups_dir = state.config.server.data_dir.join("backups");
    tokio::fs::create_dir_all(&backups_dir)
        .await
        .map_err(|e| ApiError::internal(e.into()))?;
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let filename = crate::backup::snapshot_filename(epoch);
    let dest = backups_dir.join(&filename);
    crate::backup::vacuum_into(&state.db, &dest)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope {
        data: serde_json::json!({
            "path": dest.display().to_string(),
            "filename": filename,
        }),
    }))
}

/// Serve embedded SPA assets for the admin console.
///
/// - Exact file matches (hashed JS/CSS, images) are served directly.
/// - Everything else serves `admin.html` — this gives the Admin SPA
///   client-side routing at `/` (sites, setup, dashboards).
/// - The Lobby (`index.html`) is NOT served by the console server's static
///   fallback. It is only used via `oxibuilder build` → static deployment,
///   and previewed through `--preview` mode or `/api/console/preview/...`.
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Try exact file match (hashed assets, favicon, etc.)
    if let Some(response) = serve_asset(path) {
        return response;
    }
    // Everything else → admin SPA for client-side routing
    serve_asset("admin.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn serve_asset(path: &str) -> Option<Response> {
    Assets::get(path).map(|content| {
        let mut bytes = content.data.into_owned();

        // Expose the compiled SPA revision to the browser on the console entry
        // HTML (both the exact match and the fallback path). The ErrorBoundary
        // reads this meta tag; the header is for debugging.
        if path == "admin.html" {
            let meta = format!(
                "<meta name=\"oxibuilder-spa-revision\" content=\"{}\">",
                spa_revision()
            );
            let html = String::from_utf8_lossy(&bytes);
            bytes = html
                .replace("</head>", &format!("{meta}</head>"))
                .into_bytes();
        }

        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let cache_control = cache_policy_for(path);
        let etag = format!("\"{}\"", content_hash(&bytes));

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, cache_control)
            .header(header::ETAG, etag);

        if is_html_entry(path) {
            builder = builder.header("X-Oxibuilder-SPA-Revision", spa_revision());
        }
        builder.body(Body::from(bytes)).unwrap()
    })
}

fn cache_policy_for(path: &str) -> &'static str {
    if is_html_entry(path) {
        "no-cache"
    } else if path.starts_with("assets/") && has_hash_suffix(path) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

fn is_html_entry(path: &str) -> bool {
    path == "admin.html" || path == "index.html" || path.ends_with(".html")
}

fn has_hash_suffix(path: &str) -> bool {
    // Vite emits assets/<name>-<hash>.<ext>. The first dash is the
    // name/hash separator; the hash itself is base64url and may contain
    // `-` (e.g. `main-za-BpeA9.js`, `global-GVxw7SR-.js`), so `rfind` would
    // land inside the hash and miss the separator. Use `find` and require
    // the post-separator segment to be ≥ 6 chars.
    let stem = path.strip_prefix("assets/").unwrap_or(path);
    let dot = stem.rfind('.').unwrap_or(stem.len());
    let name = &stem[..dot];
    name.find('-')
        .map(|i| &name[i + 1..])
        .is_some_and(|h| h.len() >= 6)
}

fn spa_revision() -> &'static str {
    // Compiled-in from build.rs (Task 2): the SHA-256 over the live Admin
    // embed, emitted as cargo:rustc-env=OXIBUILDER_SPA_REVISION.
    option_env!("OXIBUILDER_SPA_REVISION").unwrap_or("unknown")
}

fn content_hash(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// SSR 스냅샷용 — `web/dist/index.html`의 UTF-8 본문을 반환한다.
/// 없으면(개발 환경 등에서 `web/dist` 미빌드) `None`.
pub fn spa_index_html() -> Option<String> {
    Assets::get("index.html")
        .and_then(|f| std::str::from_utf8(f.data.as_ref()).ok().map(str::to_owned))
}

/// Iterate every embedded SPA file as `(relative_path, bytes)`. Used by `oxibuilder build`
/// to write `out/` from the embedded bundle (not the CWD `web/dist`), so a release
/// binary produces a correct site regardless of the working directory.
pub fn embedded_spa_files() -> Vec<(String, Vec<u8>)> {
    Assets::iter()
        .filter_map(|path| {
            let path = path.into_owned();
            Assets::get(&path).map(|f| (path, f.data.into_owned()))
        })
        .collect()
}

/// The static-mode SPA bundle (`embedded-spa-static`), built with `VITE_DATA_MODE=static`.
/// `oxibuilder build` writes THIS to `out/` so the deployed/previewed site reads `/data/*.json`
/// instead of hitting `/api/console` (which does not exist on a static host).
#[derive(RustEmbed)]
#[folder = "embedded-spa-static"]
struct StaticAssets;

/// Static-mode SPA `index.html` — used by `build_writer` to extract hashed asset tags.
pub fn static_spa_index_html() -> Option<String> {
    StaticAssets::get("index.html")
        .and_then(|f| std::str::from_utf8(f.data.as_ref()).ok().map(str::to_owned))
}

/// Every file in the static-mode SPA bundle. `oxibuilder build` writes these to `out/`.
pub fn static_spa_files() -> Vec<(String, Vec<u8>)> {
    StaticAssets::iter()
        .filter_map(|path| {
            let path = path.into_owned();
            StaticAssets::get(&path).map(|f| (path, f.data.into_owned()))
        })
        .collect()
}

// ─── extension lifecycle (doc/02 §2.13, doc/04 §4.3) ───

#[derive(serde::Serialize)]
struct ExtensionInfo {
    id: String,
    display_name: ManifestLocalized,
    enabled: bool,
    purged: bool,
}

async fn extension_info(state: &AppState, ext: &Arc<dyn Extension>) -> ExtensionInfo {
    let s = state.registry.status_of(ext.id()).await;
    ExtensionInfo {
        id: ext.id().to_string(),
        display_name: ManifestLocalized {
            ko: ext.display_name(Lang::Ko),
            en: ext.display_name(Lang::En),
        },
        enabled: s.map(|s| s.enabled).unwrap_or(false),
        purged: s.map(|s| s.purged).unwrap_or(false),
    }
}

async fn extensions_list(
    State(state): State<AppState>,
) -> Result<Json<DataEnvelope<Vec<ExtensionInfo>>>, ApiError> {
    let snapshot = state.registry.status_snapshot().await;
    let mut out = Vec::new();
    for e in state.registry.iter() {
        let s = snapshot.get(e.id()).copied();
        out.push(ExtensionInfo {
            id: e.id().to_string(),
            display_name: ManifestLocalized {
                ko: e.display_name(Lang::Ko),
                en: e.display_name(Lang::En),
            },
            enabled: s.map(|s| s.enabled).unwrap_or(false),
            purged: s.map(|s| s.purged).unwrap_or(false),
        });
    }
    Ok(Json(DataEnvelope { data: out }))
}

async fn extension_enable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<ExtensionInfo>>, ApiError> {
    let ext = state.registry.find(&id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "extension_not_found",
            "unknown extension id",
        )
    })?;
    let prev = state.registry.status_of(&id).await;
    let was_purged = prev.map(|s| s.purged).unwrap_or(false);
    let was_enabled = prev.map(|s| s.enabled).unwrap_or(false);
    if was_purged {
        // purge 복구: 플래그 클리어. 실제 마이그레이션은 다음 부팅 시 run_migrations가
        // schema_migrations 행 부재를 감지해 재실행한다 (run_migrations future가
        // 핸들러 컨텍스트에서 non-Send라 직접 await 불가 — 부팅 시점에 해결).
        state.registry.set_purged(&state.db, &id, false).await?;
    }
    state.registry.set_enabled(&state.db, &id, true).await?;
    if !was_enabled || was_purged {
        match ext.on_startup(&state).await {
            Ok(_) => {}
            Err(e) => tracing::warn!(extension = %id, error = %e, "on_startup failed"),
        }
    }
    Ok(Json(DataEnvelope {
        data: extension_info(&state, &ext).await,
    }))
}

async fn extension_disable(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<ExtensionInfo>>, ApiError> {
    let ext = state.registry.find(&id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "extension_not_found",
            "unknown extension id",
        )
    })?;
    let prev = state.registry.set_enabled(&state.db, &id, false).await?;
    if prev.map(|s| s.enabled).unwrap_or(false) {
        // enabled→disabled 전환 시에만 FTS 색인 즉시 정리 (doc/02 §2.13).
        match ext.on_disable(&state).await {
            Ok(_) => {}
            Err(e) => tracing::warn!(extension = %id, error = %e, "on_disable failed"),
        }
    }
    Ok(Json(DataEnvelope {
        data: extension_info(&state, &ext).await,
    }))
}

async fn extension_purge(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<serde_json::Value>>, ApiError> {
    let ext = state.registry.find(&id).ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "extension_not_found",
            "unknown extension id",
        )
    })?;
    // 1. disable + FTS 정리.
    state.registry.set_enabled(&state.db, &id, false).await?;
    let _ = ext.on_disable(&state).await;
    // 2. 확장 테이블 DROP. table_names()는 &'static str이지만 방어적으로 식별자 검증.
    for table in ext.table_names() {
        if !is_safe_ident(table) {
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "unsafe_table_name",
                "extension returned an invalid table name",
            ));
        }
        sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
            .execute(&state.db)
            .await
            .map_err(|e| ApiError::from(anyhow::anyhow!(e)))?;
    }
    // 3. 미디어 디렉토리 rm (data/media/{id}/).
    let media = state.config.server.data_dir.join("media").join(&id);
    if media.exists() {
        match std::fs::remove_dir_all(&media) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(path = %media.display(), error = %e, "failed to remove media dir during purge")
            }
        }
    }
    // 4. 마이그레이션 기록 제거 — enable-after-purge가 재실행하도록.
    sqlx::query("DELETE FROM schema_migrations WHERE extension = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::from(anyhow::anyhow!(e)))?;
    // 5. purge 플래그 세팅 (부팅 시 마이그레이션 스킵).
    state.registry.set_purged(&state.db, &id, true).await?;
    Ok(Json(DataEnvelope {
        data: serde_json::json!({ "extension_id": id, "purged": true }),
    }))
}

// ─── wasm runtime install (doc/08 §8.4) ───
//
// install 은 wasm 바이트를 data/extensions/<name>.wasm 에 저장하고 extension_state
// 행을 추가만 한다. 실제 적재(인스턴스화)는 oxibuilder-wasm 이 다음 부팅 시 수행
// (server `wasm` feature). 따라서 이 핸들러 자체는 wasmtime/oxibuilder-wasm 에 의존하지
// 않는다 — 코어 어디서나 동작.

/// 임베드된 레지스트리 카탈로그 (빌드 시점 snapshot of registry/index.json).
const REGISTRY_INDEX_JSON: &str = include_str!("../_registry.json");
/// 임베드된 데모 wasm 아티팩트 — install 오프라인 검증용 (remote 다운로드 경로와 별개).
const DEMO_WASM_BYTES: &[u8] = include_bytes!("../_wasm-demo.wasm");

/// 신뢰하는 ed25519 공개키 (base64). .wasm 아티팩트 서명 검증에 사용.
/// 프로덕션에서는 config 로 주입 가능. 데모용 고정 키.
const TRUSTED_WASM_PUBKEY_B64: &str = "ni0oDZ7ayDs26raLh0sMdQ+avv5CvF0/YUU915SzyNc=";

#[derive(serde::Deserialize)]
struct RegistryIndex {
    extensions: Vec<RegistryEntry>,
}

#[derive(serde::Deserialize)]
struct RegistryEntry {
    name: String,
    #[serde(default)]
    runtime_loadable: bool,
    #[serde(default)]
    wasm_url: Option<String>,
    /// base64 ed25519 서명. 있으면 install 시 검증.
    #[serde(default)]
    signature: Option<String>,
}

#[derive(serde::Serialize)]
struct RegistryEntryPub {
    name: String,
    runtime_loadable: bool,
    installed: bool,
    source: &'static str,
}

async fn registry_list(
    State(state): State<AppState>,
) -> Result<Json<DataEnvelope<Vec<RegistryEntryPub>>>, ApiError> {
    let index: RegistryIndex = serde_json::from_str(REGISTRY_INDEX_JSON).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "registry_error",
            &e.to_string(),
        )
    })?;
    let entries: Vec<RegistryEntryPub> = index
        .extensions
        .into_iter()
        .filter(|e| e.runtime_loadable)
        .map(|e| {
            let installed = state.registry.find(&e.name).is_some();
            let source = if e.name == "wasm-demo" {
                "embedded"
            } else {
                "remote"
            };
            RegistryEntryPub {
                name: e.name,
                runtime_loadable: true,
                installed,
                source,
            }
        })
        .collect();
    Ok(Json(DataEnvelope { data: entries }))
}

/// .wasm 바이트의 ed25519 서명을 검증. 실패 시 CONFLICT 에러.
fn verify_wasm_signature(bytes: &[u8], signature_b64: &str) -> Result<(), ApiError> {
    use base64::Engine;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let pubkey_raw = base64::engine::general_purpose::STANDARD
        .decode(TRUSTED_WASM_PUBKEY_B64)
        .map_err(|e| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "sig_key_error",
                &e.to_string(),
            )
        })?;
    let pubkey_arr: [u8; 32] = pubkey_raw.as_slice().try_into().map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "sig_key_error",
            "pubkey not 32 bytes",
        )
    })?;
    let pubkey = VerifyingKey::from_bytes(&pubkey_arr).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "sig_key_error",
            &e.to_string(),
        )
    })?;

    let sig_raw = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, "bad_signature", &e.to_string()))?;
    let sig = Signature::from_slice(&sig_raw).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "bad_signature",
            "malformed signature bytes",
        )
    })?;

    pubkey.verify(bytes, &sig).map_err(|_| {
        ApiError::new(
            StatusCode::CONFLICT,
            "signature_mismatch",
            "wasm artifact signature verification failed",
        )
    })
}

#[derive(serde::Deserialize)]
struct InstallInput {
    name: String,
}

async fn extension_install(
    State(state): State<AppState>,
    Json(input): Json<InstallInput>,
) -> Result<Json<DataEnvelope<serde_json::Value>>, ApiError> {
    let name = input.name;
    if !is_safe_extension_name(&name) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "invalid extension name",
        ));
    }
    let index: RegistryIndex = serde_json::from_str(REGISTRY_INDEX_JSON).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "registry_error",
            &e.to_string(),
        )
    })?;
    let entry = index
        .extensions
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "unknown extension name")
        })?;
    if !entry.runtime_loadable {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "not_runtime_loadable",
            "this extension is compile-time only; it cannot be installed at runtime",
        ));
    }
    // wasm 바이트 획득: 데모는 임베드, 그 외는 wasm_url 에서 다운로드.
    let bytes: Vec<u8> = if name == "wasm-demo" {
        DEMO_WASM_BYTES.to_vec()
    } else {
        let url = entry.wasm_url.as_deref().ok_or_else(|| {
            ApiError::new(
                StatusCode::CONFLICT,
                "no_wasm_url",
                "registry entry has no wasm_url",
            )
        })?;
        let resp = reqwest::get(url).await.map_err(|e| {
            ApiError::new(StatusCode::BAD_GATEWAY, "download_failed", &e.to_string())
        })?;
        if !resp.status().is_success() {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "download_failed",
                &format!("registry returned {}", resp.status()),
            ));
        }
        resp.bytes()
            .await
            .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, "download_failed", &e.to_string()))?
            .to_vec()
    };

    // 서명 검증: registry entry 에 signature 가 있으면 ed25519 검증 (§7 #5).
    if let Some(sig) = &entry.signature {
        verify_wasm_signature(&bytes, sig)?;
    }
    let dir = state.config.server.data_dir.join("extensions");
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "fs_error",
            &e.to_string(),
        )
    })?;
    let path = dir.join(format!("{name}.wasm"));
    std::fs::write(&path, &bytes).map_err(|e| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "fs_error",
            &e.to_string(),
        )
    })?;

    // 라이브 활성화: wasm_loader 가 있으면 즉시 로드·등록·활성화 (재기동 불필요).
    if let Some(loader) = &state.wasm_loader {
        match loader.load(&path) {
            Ok(ext) => {
                // extension_state 행 (enabled=1 — 라이브 활성화됨).
                sqlx::query(
                    "INSERT INTO extension_state (extension_id, enabled, purged)
                     VALUES (?1, 1, 0)
                     ON CONFLICT(extension_id) DO UPDATE SET purged = 0, enabled = 1",
                )
                .bind(&name)
                .execute(&state.db)
                .await
                .map_err(|e| ApiError::from(anyhow::anyhow!(e)))?;

                let id = state.registry.register_and_activate(ext.clone()).await;

                if let Err(e) = ext.on_startup(&state).await {
                    tracing::warn!(extension = %id, error = %e, "on_startup failed for live-loaded extension");
                }
                tracing::info!(
                    extension = %id,
                    path = %path.display(),
                    bytes = bytes.len(),
                    "installed wasm extension (live activated)"
                );
                return Ok(Json(DataEnvelope {
                    data: serde_json::json!({
                        "name": name,
                        "path": path.display().to_string(),
                        "bytes": bytes.len(),
                        "activated": true,
                    }),
                }));
            }
            Err(e) => {
                tracing::warn!(
                    extension = %name,
                    error = %e,
                    "wasm live-activation failed — restart to retry"
                );
            }
        }
    }

    // extension_state 행 (enabled=0). wasm_loader 가 없거나 로드 실패 → 재기동 필요.
    sqlx::query(
        "INSERT INTO extension_state (extension_id, enabled, purged)
         VALUES (?1, 0, 0)
         ON CONFLICT(extension_id) DO UPDATE SET purged = 0",
    )
    .bind(&name)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::from(anyhow::anyhow!(e)))?;
    tracing::info!(
        extension = %name,
        path = %path.display(),
        bytes = bytes.len(),
        "installed wasm extension (restart to activate)"
    );
    Ok(Json(DataEnvelope {
        data: serde_json::json!({
            "name": name,
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "activated": false,
            "note": "restart oxibuilder-console (built with --features wasm) to activate",
        }),
    }))
}

fn is_safe_ident(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 런타임 설치 확장 이름 검증. is_safe_ident 와 달리 하이픈을 허용한다 —
/// 이름은 파일명(data/extensions/<name>.wasm)과 매개변수화 SQL 바인드에만 쓰이므로
/// 하이픈이 안전하다 (경로 순회 차단: `/` `\` `.` 는 여전히 거부). SQL 식별자로
/// 보간되는 table_names() 에는 이 함수를 쓰면 안 된다 — is_safe_ident 를 쓸 것.
fn is_safe_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `/api/console/{ext}/**` 경로에서 ext 세그먼트 추출. 코어 라우트(lobby/auth/search/docs)는
/// registry.find 가 None이므로 게이트 대상이 아니다.
fn extension_id_from_path(path: &str) -> Option<String> {
    // nest 내부 layer라 path는 /api/console prefix가 벗겨진 상태다 ("/dummy", "/lobby/manifest").
    let seg = path.trim_start_matches('/').split('/').next()?;
    if seg.is_empty() {
        None
    } else {
        Some(seg.to_string())
    }
}

/// 런타임 게이트 미들웨어. registry에 있는 확장이 비활성/purged면 404.
async fn extension_gate(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if let Some(ext_id) = extension_id_from_path(request.uri().path())
        && state.registry.find(&ext_id).is_some()
        && !state.registry.is_active(&ext_id).await
    {
        return ApiError::new(
            StatusCode::NOT_FOUND,
            "extension_disabled",
            "extension is disabled or purged",
        )
        .into_response();
    }
    next.run(request).await
}

// ─── CLI 동적 명령 (doc/11) ───

async fn cli_commands_handler(State(state): State<AppState>) -> Json<CliCommandManifest> {
    // 활성 확장 상태 스냅샷을 미리 수집 (async boundary)
    let statuses = state.registry.status_snapshot().await;

    let extensions: Vec<CliCommandSpec> = state
        .registry
        .iter()
        .into_iter()
        .filter(|ext| statuses.get(ext.id()).map(|s| s.active()).unwrap_or(false))
        .flat_map(|ext| {
            let id = ext.id().to_string();
            ext.cli_commands()
                .into_iter()
                .map(move |cmd| CliCommandSpec {
                    extension_id: id.clone(),
                    name: cmd.name.to_string(),
                    about: cmd.about.to_string(),
                    subcommands: cmd
                        .subcommands
                        .into_iter()
                        .map(|sub| CliSubcommandSpec {
                            name: sub.name.to_string(),
                            about: sub.about.to_string(),
                            args: sub
                                .args
                                .into_iter()
                                .map(|a| CliArgSpec {
                                    long: a.long.to_string(),
                                    short: a.short,
                                    help: a.help.to_string(),
                                    required: a.required,
                                })
                                .collect(),
                        })
                        .collect(),
                })
        })
        .collect();
    Json(CliCommandManifest { extensions })
}

#[derive(serde::Deserialize)]
struct CliExecInput {
    args: std::collections::BTreeMap<String, String>,
}

/// WASM 확장이 핸들러 없는 CLI 명령을 위임받아 실행.
/// 핸들러가 None인 확장의 CLI 명령은 이 엔드포인트로 요청이 온다.
/// 컴파일 확장은 직접 `CliHandler`를 가지므로 이 경로를 거치지 않는다.
async fn cli_exec_handler(
    State(state): State<AppState>,
    Path((ext_id, sub_command)): Path<(String, String)>,
    Json(input): Json<CliExecInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ext = state.registry.find(&ext_id).ok_or_else(|| {
        let msg = format!("extension '{ext_id}' not found");
        ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "extension_not_found",
            &msg,
        )
    })?;

    // 확장의 cli_commands()에서 서브커맨드 찾기
    let cmd = ext
        .cli_commands()
        .into_iter()
        .find_map(|c| c.subcommands.into_iter().find(|s| s.name == sub_command))
        .ok_or_else(|| {
            let msg = format!("subcommand '{sub_command}' not found in extension '{ext_id}'");
            ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "subcommand_not_found",
                &msg,
            )
        })?;

    // 핸들러가 있으면 서버에서 실행할 수 없다 — 컴파일 확장 전용
    if cmd.handler.is_some() {
        let msg =
            format!("command '{ext_id} {sub_command}' has a native handler and cannot be proxied");
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "handler_not_proxyable",
            &msg,
        ));
    }

    // TODO: WASM 확장은 여기서 arg 검증 후 적절한 API 호출로 변환.
    // 현재 Phase 1: stub — args를 그대로 반영해 echo
    tracing::info!(
        "cli exec proxy: ext={ext_id} sub={sub_command} args={:?}",
        input.args
    );
    Ok(Json(serde_json::json!({
        "status": "stub",
        "ext_id": ext_id,
        "sub_command": sub_command,
        "args": input.args,
    })))
}

// ─── theme (doc/12 §12.7) ───

/// `GET /api/console/themes` — public catalog. Auth-free; used by the
/// web SPA to enumerate available themes. Each entry is serialized with
/// the SAME flat shape as the per-site and default-site theme endpoints
/// (`{ id, name_ko, name_en, mode, accent_hue, preview_colors,
/// description_ko, description_en }`). The browser `ThemeDefinition`
/// type matches all three endpoints, so no shape translation is needed.
async fn theme_catalog() -> Json<DataEnvelope<Vec<theme::ThemeDefinition>>> {
    Json(DataEnvelope {
        data: theme::ALL_THEMES.to_vec(),
    })
}
