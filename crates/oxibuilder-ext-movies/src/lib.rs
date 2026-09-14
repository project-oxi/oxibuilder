pub mod integration;
pub mod model;
pub mod repo;
pub mod routes;

use async_trait::async_trait;
use axum::Router;
use axum::routing::{get, post};
use oxibuilder_core::client::Client;

use oxibuilder_core::builder::{BuildExt, SearchDoc, StaticPage};
use oxibuilder_core::extension::{
    CliArg, CliCommand, CliHandler, CliSubcommand, Extension, ExtensionWizard, Lang, LobbyCard,
    LobbyCardItem, Migration, SetupField, SetupFieldKind, SetupSaveHandler, SetupStep, StepOutcome,
    VisibilityRule, persist_extension_config,
};
use oxibuilder_core::state::AppState;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::error::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct MoviesExtension;

// ── CLI handlers ──

/// 기본 사이트 슬러그 해석 — 콘텐츠 라우트는 `/api/console/s/{slug}/...` 로만 노출된다.
async fn resolve_site(client: &Client) -> anyhow::Result<String> {
    let v = client
        .get("/api/console/sites/default")
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    v.get("data")
        .and_then(|d| d.get("default_site"))
        .and_then(|s| s.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no default site configured"))
}

struct MovieAddHandler;
impl CliHandler for MovieAddHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        // media_type / rating 은 NOT NULL 이므로 기본값을 채운다.
        let media_type = args
            .get("media-type")
            .cloned()
            .unwrap_or_else(|| "movie".to_string());
        let rating = args
            .get("rating")
            .cloned()
            .unwrap_or_else(|| "0".to_string());
        let mut body = serde_json::json!({ "media_type": media_type, "rating": rating });
        if let Some(t) = args.get("title") {
            body["title"] = serde_json::json!(t);
        }
        if let Some(id) = args.get("tmdb-id")
            && let Ok(n) = id.parse::<i64>()
        {
            body["tmdb_id"] = serde_json::json!(n);
        }
        if let Some(slug) = args.get("slug") {
            body["slug"] = serde_json::json!(slug);
        }
        let client = client.clone();
        Box::pin(async move {
            let site = resolve_site(&client).await?;
            let resp = client
                .post(&format!("/api/console/s/{site}/movies/"), &body)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        })
    }
}

struct MovieSearchHandler;
impl CliHandler for MovieSearchHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let q = args.get("query").cloned().unwrap_or_default();
        let client = client.clone();
        Box::pin(async move {
            if q.trim().is_empty() {
                anyhow::bail!("--query/-q is required");
            }
            let site = resolve_site(&client).await?;
            // 웹 UI 의 TmdbSearchRow 와 동일한 백엔드 엔드포인트 (단일 검색 소스).
            let resp = client
                .get(&format!(
                    "/api/console/s/{site}/movies/search?q={}",
                    pct(&q)
                ))
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let results = resp
                .get("data")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            if results.is_empty() {
                println!("No results for \"{q}\".");
                return Ok(());
            }
            for r in &results {
                let id = r.get("tmdb_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let title = r
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(untitled)");
                let year = r.get("release_year").and_then(|v| v.as_i64());
                println!(
                    "[{id}] {title}{}",
                    year.map(|y| format!(" ({y})")).unwrap_or_default()
                );
            }
            println!("\nAdd a result: oxibuilder movies add --tmdb-id <ID> [--rating N]");
            Ok(())
        })
    }
}

/// 검색어 percent-encoding (한글 멀티바이트 포함). std 만으로 구현.
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

struct MovieSeriesCreateHandler;
impl CliHandler for MovieSeriesCreateHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let name = args.get("name").cloned().unwrap_or_default();
        let slug = args.get("slug").cloned().unwrap_or_default();
        // SeriesGroupInput 은 title_ko/title_en 을 받는다 (--name → title_ko).
        let body = serde_json::json!({ "title_ko": name, "slug": slug });
        let client = client.clone();
        Box::pin(async move {
            let site = resolve_site(&client).await?;
            let resp = client
                .post(&format!("/api/console/s/{site}/movies/series"), &body)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        })
    }
}

struct MovieRefreshHandler;
impl CliHandler for MovieRefreshHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        // --slug <slug> (단건) 또는 --all (전체). 하나만 지정해야 한다.
        let slug = args.get("slug").cloned();
        let all = args.get("all").map(|v| v == "true").unwrap_or(false);
        let client = client.clone();
        Box::pin(async move {
            match (slug, all) {
                (Some(s), false) => {
                    let resp = client
                        .post(
                            &format!("/api/console/movies/{s}/refresh"),
                            &serde_json::json!({}),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
                (None, true) => {
                    // 모든 엔트리를 순회. 단건 실패는 로깅 후 계속.
                    let list_resp = client
                        .get("/api/console/movies/?limit=200")
                        .await
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let items = list_resp
                        .get("data")
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut refreshed = 0usize;
                    let mut skipped = 0usize;
                    for item in &items {
                        let slug = match item.get("slug").and_then(|v| v.as_str()) {
                            Some(s) => s.to_string(),
                            None => continue,
                        };
                        match client
                            .post(
                                &format!("/api/console/movies/{slug}/refresh"),
                                &serde_json::json!({}),
                            )
                            .await
                        {
                            Ok(_) => refreshed += 1,
                            Err(e) => {
                                tracing::warn!(slug = %slug, error = %e, "refresh failed; skipping");
                                skipped += 1;
                            }
                        }
                    }
                    println!(
                        "refreshed {refreshed} entries, skipped {skipped} (of {} total)",
                        items.len()
                    );
                }
                _ => {
                    anyhow::bail!("specify exactly one of --slug <slug> or --all");
                }
            }
            Ok(())
        })
    }
}

struct MoviesKeySave;
#[async_trait]
impl SetupSaveHandler for MoviesKeySave {
    async fn save(
        &self,
        ctx: &AppState,
        form: &serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<StepOutcome> {
        if let Some(v) = form.get("tmdb_key").and_then(|x| x.as_str())
            && !v.is_empty()
        {
            // SAFETY: setup wizard 는 단일 사용자 로컬 환경에서만 동작.
            unsafe {
                std::env::set_var("OXIBUILDER_TMDB_KEY", v);
            }
            persist_extension_config(ctx, "movies", "OXIBUILDER_TMDB_KEY", v).await?;
        }
        Ok(StepOutcome::from_form(form))
    }
}
#[async_trait]
impl Extension for MoviesExtension {
    fn id(&self) -> &'static str {
        "movies"
    }
    fn table_names(&self) -> Vec<&'static str> {
        vec!["series_group", "movie_entry"]
    }

    fn display_name(&self, lang: Lang) -> String {
        match lang {
            Lang::Ko => "영화".to_string(),
            Lang::En => "Movies".to_string(),
        }
    }

    fn migrations(&self) -> Vec<Migration> {
        vec![
            Migration {
                version: 1,
                name: "init",
                sql: include_str!("../migrations/0001_init.sql"),
            },
            Migration {
                version: 2,
                name: "meta",
                sql: include_str!("../migrations/0002_meta.sql"),
            },
            Migration {
                version: 3,
                name: "origin",
                sql: include_str!("../migrations/0003_origin.sql"),
            },
        ]
    }

    fn routes(&self) -> Router {
        Router::new()
            .route("/", get(routes::list).post(routes::create))
            .route("/search", get(routes::tmdb_search))
            .route(
                "/{slug}",
                get(routes::show)
                    .patch(routes::update)
                    .delete(routes::delete),
            )
            .route("/{slug}/publish", post(routes::publish))
            .route("/{slug}/refresh", post(routes::refresh))
            .route(
                "/series",
                get(routes::list_groups).post(routes::create_group),
            )
            .route(
                "/series/{slug}",
                get(routes::show_group)
                    .patch(routes::update_group)
                    .delete(routes::delete_group),
            )
    }

    async fn lobby_summary(&self, ctx: &AppState) -> Option<LobbyCard> {
        let entries = repo::list_entries_published(&ctx.db, None, 3).await.ok()?;
        let items = entries
            .into_iter()
            .map(|e| LobbyCardItem {
                title: e.display_title().to_string(),
                url: format!("/movies/{}", e.slug),
            })
            .collect();
        Some(LobbyCard {
            id: self.id().to_string(),
            items,
        })
    }

    fn cli_commands(&self) -> Vec<CliCommand> {
        vec![CliCommand {
            name: "movies",
            about: "Manage movies and series",
            subcommands: vec![
                CliSubcommand {
                    name: "add",
                    about: "Add a movie review",
                    args: vec![
                        CliArg {
                            long: "tmdb-id",
                            short: None,
                            help: "TMDB movie id (auto-fills bilingual title, genres, cast)",
                            required: false,
                        },
                        CliArg {
                            long: "title",
                            short: Some('t'),
                            help: "Movie title (manual; or use --tmdb-id)",
                            required: false,
                        },
                        CliArg {
                            long: "media-type",
                            short: Some('m'),
                            help: "movie | tv (default: movie)",
                            required: false,
                        },
                        CliArg {
                            long: "slug",
                            short: Some('s'),
                            help: "URL slug",
                            required: false,
                        },
                        CliArg {
                            long: "rating",
                            short: Some('r'),
                            help: "Rating (0-10)",
                            required: false,
                        },
                    ],
                    handler: Some(Arc::new(MovieAddHandler)),
                },
                CliSubcommand {
                    name: "search",
                    about: "Search TMDB (same source as the web UI)",
                    args: vec![CliArg {
                        long: "query",
                        short: Some('q'),
                        help: "Search query",
                        required: true,
                    }],
                    handler: Some(Arc::new(MovieSearchHandler)),
                },
                CliSubcommand {
                    name: "series",
                    about: "Create a series group",
                    args: vec![
                        CliArg {
                            long: "name",
                            short: Some('n'),
                            help: "Series name",
                            required: true,
                        },
                        CliArg {
                            long: "slug",
                            short: Some('s'),
                            help: "URL slug",
                            required: true,
                        },
                    ],
                    handler: Some(Arc::new(MovieSeriesCreateHandler)),
                },
                CliSubcommand {
                    name: "refresh",
                    about: "Re-fetch movie metadata (origin) for an entry or --all",
                    args: vec![
                        CliArg {
                            long: "slug",
                            short: Some('s'),
                            help: "Movie slug to refresh (mutually exclusive with --all)",
                            required: false,
                        },
                        CliArg {
                            long: "all",
                            short: None,
                            help: "Refresh every entry (per-entry failures are logged and skipped)",
                            required: false,
                        },
                    ],
                    handler: Some(Arc::new(MovieRefreshHandler)),
                },
            ],
        }]
    }

    fn setup_wizard(&self) -> Option<ExtensionWizard> {
        Some(ExtensionWizard {
            steps: vec![
                SetupStep {
                    id: "movies_key",
                    title_ko: "TMDB API 키",
                    title_en: "TMDB API key",
                    description_ko: "영화 정보 연동을 위한 TMDB 키 (선택)",
                    description_en: "TMDB key for movie data (optional)",
                    fields: vec![SetupField {
                        name: "tmdb_key",
                        label_ko: "TMDB API 키",
                        label_en: "TMDB API key",
                        kind: SetupFieldKind::Secret,
                        required: false,
                        placeholder_ko: None,
                        placeholder_en: None,
                    }],
                    save_handler: Arc::new(MoviesKeySave),
                    prefill: BTreeMap::new(),
                    visible_when: None,
                },
                SetupStep {
                    id: "movies_test",
                    title_ko: "TMDB 연결 테스트",
                    title_en: "TMDB connection test",
                    description_ko: "입력한 키로 TMDB 에 접근되는지 확인합니다",
                    description_en: "Verify the key can reach TMDB",
                    fields: vec![],
                    save_handler: Arc::new(MoviesTestSave),
                    prefill: BTreeMap::new(),
                    visible_when: Some(VisibilityRule::FieldNotEmpty {
                        step_id: "movies_key",
                        field: "tmdb_key",
                    }),
                },
            ],
        })
    }
}

struct MoviesTestSave;
#[async_trait]
impl SetupSaveHandler for MoviesTestSave {
    async fn save(
        &self,
        _ctx: &AppState,
        _form: &serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<StepOutcome> {
        let tmdb = integration::TmdbClient::from_env();
        let ok = if tmdb.enabled() {
            tmdb.search("test").await.is_ok()
        } else {
            false
        };
        let mut m = serde_json::Map::new();
        m.insert(
            "connection_ok".into(),
            if ok { "true" } else { "false" }.into(),
        );
        Ok(StepOutcome { values: m })
    }
}
impl BuildExt for MoviesExtension {
    fn ext_id(&self) -> &'static str {
        "movies"
    }

    fn build_pages(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<StaticPage>, Box<dyn Error + Send + Sync>> {
        let details = rt.block_on(repo::list_entries_detail(db, 1000, false))?;
        let mut pages = Vec::with_capacity(details.len());
        for d in &details {
            let title = d.entry.display_title();
            let excerpt: String = d
                .entry
                .review_ko
                .as_deref()
                .or(d.entry.review_en.as_deref())
                .unwrap_or("")
                .chars()
                .take(160)
                .collect();
            pages.push(StaticPage {
                path: format!("movies/{}/index.html", d.entry.slug),
                content: format!(
                    r#"<!DOCTYPE html><html lang="ko"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
    <title>{title}</title><meta property="og:title" content="{title}"><meta property="og:description" content="{excerpt}">
    <meta property="og:type" content="website"><meta property="og:url" content="/movies/{slug}/">
    <link rel="canonical" href="/movies/{slug}/"></head><body><div id="root"></div><script src="/assets/index.js"></script></body></html>"#,
                    title=title, excerpt=excerpt, slug=d.entry.slug),
            });
        }
        Ok(pages)
    }

    fn build_data(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn Error + Send + Sync>> {
        // 장르·출연진·현지화 제목까지 포함된 상세를 내보낸다 (공개 SPA data/movies.json).
        let details = rt.block_on(repo::list_entries_detail(db, 1000, false))?;
        Ok(Box::new(details))
    }

    fn build_search_docs(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<SearchDoc>, Box<dyn Error + Send + Sync>> {
        let details = rt.block_on(repo::list_entries_detail(db, 1000, false))?;
        Ok(details
            .into_iter()
            .map(|d| {
                let title = d.entry.display_title().to_string();
                let body = d.entry.review_ko.or(d.entry.review_en).unwrap_or_default();
                let excerpt: String = body.chars().take(200).collect();
                SearchDoc {
                    id: format!("movies/{}", d.entry.slug),
                    title,
                    body_preview: excerpt,
                    doc_type: "movies".into(),
                    url: format!("/movies/{}", d.entry.slug),
                    published_at: d.entry.published_at,
                }
            })
            .collect())
    }

    fn external_image_urls(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        // TMDB `poster_path` is stored as a raw path (e.g. "/abc.jpg"), not a full URL.
        // Build the full `w500` URL here so the manifest key matches what `optimize_external`
        // will fetch and (later) what the SPA pre-pass substitutes for `MovieCard.posterUrl()`.
        const POSTER_BASE: &str = "https://image.tmdb.org/t/p/w500";
        let rows: Vec<(Option<String>,)> = rt.block_on(async {
            sqlx::query_as(
                "SELECT poster_path FROM movie_entry \
                 WHERE published_at IS NOT NULL AND poster_path IS NOT NULL \
                   AND poster_path <> ''",
            )
            .fetch_all(db)
            .await
        })?;
        Ok(rows
            .into_iter()
            .filter_map(|(p,)| p)
            .filter(|p| !p.is_empty())
            // Multi-segment absolute paths are local mounted refs
            // (e.g. "/posters/x.jpg") — served as-is by static mounts, not
            // fetchable TMDB URLs. Skip them so optimize_external doesn't
            // waste a per-URL timeout on each.
            .filter(|p| !(p.starts_with('/') && p[1..].contains('/')))
            .map(|p| format!("{POSTER_BASE}{p}"))
            .collect())
    }
}
