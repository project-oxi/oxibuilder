use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::Extension;
use axum::http::{Request, StatusCode, header::AUTHORIZATION};
use oxibuilder_core::config::Config;
use oxibuilder_core::registry::ExtensionRegistry;
use oxibuilder_core::state::SiteScopedDb;
use oxibuilder_ext_blog::BlogExtension;
use std::sync::Arc;
use tower::ServiceExt;

async fn test_app(_admin_token: Option<&str>) -> Router {
    let pool = oxibuilder_core::db::connect_memory().await.unwrap();
    let registry = Arc::new(ExtensionRegistry::new(vec![Arc::new(BlogExtension::new())]));
    registry.run_migrations(&pool, &[]).await.unwrap();
    // Blog extension's on_startup still needs AppState, create minimally
    let state = oxibuilder_core::state::AppState {
        db: pool.clone(),
        config: Arc::new(Config::default()),
        registry: registry.clone(),
        wasm_loader: None,
        site_override: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        builders: std::sync::Arc::new(vec![]),
    };
    for e in registry.iter() {
        e.on_startup(&state).await.unwrap();
    }
    let ext_router = registry.find("blog").unwrap().routes();
    Router::new()
        .nest("/api/console/blog", ext_router)
        .layer(Extension(SiteScopedDb {
            db: pool,
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(
                oxibuilder_core::site_paths::MutableSiteSettings::from_config(
                    &oxibuilder_core::config::Config::default(),
                ),
            )),
        }))
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn create_with_empty_title_is_422() {
    let app = test_app(Some("tok")).await;
    let res = app
        .oneshot(
            Request::post("/api/console/blog")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::from(r#"{"title":"  "}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_invalid_lang_is_422() {
    let app = test_app(Some("tok")).await;
    let res = app
        .oneshot(
            Request::post("/api/console/blog")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::from(r#"{"title":"x","lang":"ja"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn draft_create_then_publish_flow() {
    let app = test_app(Some("tok")).await;
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/console/blog")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::from(
                    r##"{"title":"Hello Rust","body":"# body","lang":"en","tags":["rust"]}"##,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    let slug = json["data"]["slug"].as_str().unwrap().to_string();
    assert!(json["data"]["published_at"].is_null());

    // 초안은 공개 show에서 404
    let res = app
        .clone()
        .oneshot(
            Request::get(format!("/api/console/blog/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 발행본 목록은 비어 있음
    let res = app
        .clone()
        .oneshot(
            Request::get("/api/console/blog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(res).await;
    assert_eq!(json["data"].as_array().unwrap().len(), 0);

    // 초안 목록에는 1개
    let res = app
        .clone()
        .oneshot(
            Request::get("/api/console/blog?draft=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(res).await;
    assert_eq!(json["data"].as_array().unwrap().len(), 1);

    // 발행
    let res = app
        .clone()
        .oneshot(
            Request::post(format!("/api/console/blog/{slug}/publish"))
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    assert!(json["data"]["published_at"].is_string());

    // 이제 show가 200
    let res = app
        .clone()
        .oneshot(
            Request::get(format!("/api/console/blog/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 발행본 목록에 1개
    let res = app
        .oneshot(
            Request::get("/api/console/blog")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let json = body_json(res).await;
    assert_eq!(json["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn fts_index_on_publish() {
    let pool = oxibuilder_core::db::connect_memory().await.unwrap();
    let registry = Arc::new(ExtensionRegistry::new(vec![Arc::new(BlogExtension::new())]));
    registry.run_migrations(&pool, &[]).await.unwrap();

    let _draft = oxibuilder_ext_blog::repo::create(
        &pool,
        &oxibuilder_ext_blog::model::BlogPostInput {
            title: "Rust Ownership".into(),
            body: "Ownership borrowing lifetime".into(),
            lang: "en".into(),
            tags: vec![],
            translation_group_id: None,
            slug: Some("rust-ownership".into()),
            category: None,
            series: None,
            series_order: None,
        },
        "rust-ownership",
    )
    .await
    .unwrap();
    let post = oxibuilder_ext_blog::repo::publish(&pool, "rust-ownership")
        .await
        .unwrap();
    oxibuilder_core::search::upsert(
        &pool,
        "blog",
        &post.slug,
        &post.title,
        &post.body,
        Some(&post.lang),
        post.published_at.as_deref(),
    )
    .await
    .unwrap();

    let hits = oxibuilder_core::search::search(&pool, "ownership", None, 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].doc_id, "rust-ownership");
    let hits = oxibuilder_core::search::search(&pool, "ownership", None, 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].doc_id, "rust-ownership");
}

/// SSR 스냅샷 호출은 best-effort이라 file 시스템 검증은 회피하고,
/// publish가 200 응답으로 정상 완료되는지만 확인한다.
/// (실제 file 생성은 `snapshot::write_snapshot_for` 단위 테스트가 보장).
#[tokio::test]
async fn publish_does_not_block_on_ssr_failure() {
    let app = test_app(Some("tok")).await;
    // 초안 생성
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/console/blog")
                .header("content-type", "application/json")
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::from(
                    r##"{"title":"Snapshot Test","body":"body content here","lang":"ko"}"##,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    let slug = json["data"]["slug"].as_str().unwrap().to_string();

    // SSR 보조 호출이 실패(예: index.html 미임베드)해도 publish API는 200을 반환해야 한다.
    let res = app
        .oneshot(
            Request::post(format!("/api/console/blog/{slug}/publish"))
                .header(AUTHORIZATION, bearer("tok"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    assert!(json["data"]["published_at"].is_string());
}

#[tokio::test]
async fn translation_group_is_shared_and_promoted_from_original() {
    let pool = oxibuilder_core::db::connect_memory().await.unwrap();
    let registry = Arc::new(ExtensionRegistry::new(vec![Arc::new(BlogExtension::new())]));
    registry.run_migrations(&pool, &[]).await.unwrap();

    let ko = oxibuilder_ext_blog::repo::create(
        &pool,
        &oxibuilder_ext_blog::model::BlogPostInput {
            title: "원본".into(),
            body: String::new(),
            lang: "ko".into(),
            tags: vec![],
            translation_group_id: None,
            slug: None,
            category: None,
            series: None,
            series_order: None,
        },
        "wonbon",
    )
    .await
    .unwrap();
    assert_eq!(ko.translation_group_id, None);

    // 번역 생성 전 group 확정 — 원본 승격: group = 원본 id.
    let group = oxibuilder_ext_blog::repo::translation_group_for(&pool, "wonbon")
        .await
        .unwrap();
    assert_eq!(group, ko.id);

    let en = oxibuilder_ext_blog::repo::create(
        &pool,
        &oxibuilder_ext_blog::model::BlogPostInput {
            title: "Original".into(),
            body: String::new(),
            lang: "en".into(),
            tags: vec![],
            translation_group_id: Some(group),
            slug: None,
            category: None,
            series: None,
            series_order: None,
        },
        "original",
    )
    .await
    .unwrap();

    // 원본도 이제 그룹에 속하며, 재확정은 같은 group을 반환한다 (멱등).
    let ko = oxibuilder_ext_blog::repo::find_by_slug(&pool, "wonbon")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ko.translation_group_id, Some(group));
    assert_eq!(
        oxibuilder_ext_blog::repo::translation_group_for(&pool, "original")
            .await
            .unwrap(),
        group
    );

    let siblings = oxibuilder_ext_blog::repo::list_translations(&pool, group, "original")
        .await
        .unwrap();
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].slug, "wonbon");
}

#[tokio::test]
async fn series_orders_published_posts() {
    let pool = oxibuilder_core::db::connect_memory().await.unwrap();
    let registry = Arc::new(ExtensionRegistry::new(vec![Arc::new(BlogExtension::new())]));
    registry.run_migrations(&pool, &[]).await.unwrap();

    let series = oxibuilder_ext_blog::repo::series_create(
        &pool,
        &oxibuilder_ext_blog::model::BlogSeriesInput {
            title: "Rust 배우기".into(),
            description: "입문 시리즈".into(),
            slug: Some("rust".into()),
        },
    )
    .await
    .unwrap();

    let make = |title: &str, slug: &str, order: Option<i64>| oxibuilder_ext_blog::model::BlogPostInput {
        title: title.into(),
        body: String::new(),
        lang: "ko".into(),
        tags: vec![],
        translation_group_id: None,
        slug: Some(slug.into()),
        category: Some("개발".into()),
        series: Some("rust".into()),
        series_order: order,
    };
    let one = oxibuilder_ext_blog::repo::create(&pool, &make("첫화", "ep1", Some(1)), "ep1")
        .await
        .unwrap();
    let _ = oxibuilder_ext_blog::repo::create(&pool, &make("세화", "ep3", Some(3)), "ep3")
        .await
        .unwrap();
    let _ = oxibuilder_ext_blog::repo::create(&pool, &make("초안화", "ep-draft", Some(2)), "ep-draft")
        .await
        .unwrap();
    oxibuilder_ext_blog::repo::publish(&pool, "ep1").await.unwrap();
    oxibuilder_ext_blog::repo::publish(&pool, "ep3").await.unwrap();

    // 미발행 초안은 시리즈 목록에서 제외된다.
    let posts = oxibuilder_ext_blog::repo::series_posts(&pool, series.id)
        .await
        .unwrap();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].slug, "ep1");
    assert_eq!(posts[0].category.as_deref(), Some("개발"));
    assert_eq!(posts[1].slug, "ep3");

    // 순서 미지정 글은 맨 뒤로 간다.
    let _ = oxibuilder_ext_blog::repo::create(&pool, &make("맨뒤", "ep-last", None), "ep-last")
        .await
        .unwrap();
    oxibuilder_ext_blog::repo::publish(&pool, "ep-last").await.unwrap();
    let posts = oxibuilder_ext_blog::repo::series_posts(&pool, series.id)
        .await
        .unwrap();
    assert_eq!(posts.last().unwrap().slug, "ep-last");

    // 존재하지 않는 시리즈 지정은 에러.
    let mut bad = make("x", "ep-x", None);
    bad.series = Some("없는시리즈".into());
    assert!(oxibuilder_ext_blog::repo::create(&pool, &bad, "ep-x").await.is_err());

    // publish는 translation_test와 무관하게 id를 소진하지 않는다.
    let _ = one;
}

#[tokio::test]
async fn update_partial_fields_via_positional_binds() {
    let pool = oxibuilder_core::db::connect_memory().await.unwrap();
    let registry = Arc::new(ExtensionRegistry::new(vec![Arc::new(BlogExtension::new())]));
    registry.run_migrations(&pool, &[]).await.unwrap();
    let _ = oxibuilder_ext_blog::repo::series_create(
        &pool,
        &oxibuilder_ext_blog::model::BlogSeriesInput {
            title: "시리즈".into(),
            description: String::new(),
            slug: Some("s".into()),
        },
    )
    .await
    .unwrap();

    let input = |title: &str| oxibuilder_ext_blog::model::BlogPostInput {
        title: title.into(),
        body: String::new(),
        lang: "ko".into(),
        tags: vec![],
        translation_group_id: None,
        slug: Some(title.into()),
        category: None,
        series: None,
        series_order: None,
    };
    let _ = oxibuilder_ext_blog::repo::create(&pool, &input("t1"), "t1")
        .await
        .unwrap();

    // 제목 + 카테고리 동시 수정 — 위치 바인드 순서 검증.
    let patched = oxibuilder_ext_blog::repo::update(
        &pool,
        "t1",
        &oxibuilder_ext_blog::model::BlogPatch {
            title: Some("제목변경".into()),
            category: Some(Some("개발".into())),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(patched.title, "제목변경");
    assert_eq!(patched.category.as_deref(), Some("개발"));

    // 시리즈 지정 + 순서 지정.
    let patched = oxibuilder_ext_blog::repo::update(
        &pool,
        "t1",
        &oxibuilder_ext_blog::model::BlogPatch {
            series: Some(Some("s".into())),
            series_order: Some(Some(2)),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(patched.series_id, Some(1));
    assert_eq!(patched.series_order, Some(2));

    // 카테고리 해제 (Some(None) → NULL).
    let patched = oxibuilder_ext_blog::repo::update(
        &pool,
        "t1",
        &oxibuilder_ext_blog::model::BlogPatch {
            category: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(patched.category, None);
    assert_eq!(patched.series_order, Some(2)); // 미지정 필드는 보존
}
