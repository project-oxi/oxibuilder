pub mod client;
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
use std::pin::Pin;
use std::sync::Arc;

pub struct BooksExtension;

// ── CLI handlers ──

struct BookAddHandler;
impl CliHandler for BookAddHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        let title = args.get("title").cloned().unwrap_or_default();
        let mut body = serde_json::json!({ "title": title });
        if let Some(author) = args.get("author") {
            body["author"] = serde_json::json!(author);
        }
        if let Some(rating) = args.get("rating") {
            body["rating"] = serde_json::json!(rating);
        }
        let client = client.clone();
        Box::pin(async move {
            let resp = client
                .post("/api/console/books/", &body)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        })
    }
}

struct BookRefreshHandler;
impl CliHandler for BookRefreshHandler {
    fn run(
        &self,
        args: BTreeMap<String, String>,
        client: &Client,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>> {
        // --id <id> (단건) 또는 --all (전체). 하나만 지정해야 한다.
        let id = args.get("id").cloned();
        let all = args.get("all").map(|v| v == "true").unwrap_or(false);
        let client = client.clone();
        Box::pin(async move {
            match (id, all) {
                (Some(i), false) => {
                    let resp = client
                        .post(
                            &format!("/api/console/books/{i}/refresh"),
                            &serde_json::json!({}),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!("{}", serde_json::to_string_pretty(&resp)?);
                }
                (None, true) => {
                    // 모든 엔트리를 순회. 단건 실패는 로깅 후 계속.
                    let list_resp = client
                        .get("/api/console/books/?limit=200")
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
                        let id = match item.get("id").and_then(|v| v.as_i64()) {
                            Some(id) => id,
                            None => continue,
                        };
                        match client
                            .post(
                                &format!("/api/console/books/{id}/refresh"),
                                &serde_json::json!({}),
                            )
                            .await
                        {
                            Ok(_) => refreshed += 1,
                            Err(e) => {
                                tracing::warn!(id, error = %e, "refresh failed; skipping");
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
                    anyhow::bail!("specify exactly one of --id <id> or --all");
                }
            }
            Ok(())
        })
    }
}

#[async_trait]
impl Extension for BooksExtension {
    fn id(&self) -> &'static str {
        "books"
    }
    fn table_names(&self) -> Vec<&'static str> {
        vec!["book_entry"]
    }

    fn display_name(&self, lang: Lang) -> String {
        match lang {
            Lang::Ko => "책".to_string(),
            Lang::En => "Books".to_string(),
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
                name: "metadata",
                sql: include_str!("../migrations/0002_metadata.sql"),
            },
        ]
    }

    fn routes(&self) -> Router {
        Router::new()
            .route("/", get(routes::list).post(routes::create))
            .route("/search", get(routes::external_search))
            .route(
                "/{id}",
                get(routes::show)
                    .patch(routes::update)
                    .delete(routes::delete),
            )
            .route("/{id}/publish", post(routes::publish))
            .route("/{id}/refresh", post(routes::refresh))
    }

    async fn lobby_summary(&self, ctx: &AppState) -> Option<LobbyCard> {
        let recent = repo::list(&ctx.db, None, 3, false).await.ok()?;
        if recent.is_empty() {
            return None;
        }
        let items = recent
            .into_iter()
            .map(|b| LobbyCardItem {
                title: b.title,
                url: format!("/books/{}", b.id),
            })
            .collect();
        Some(LobbyCard {
            id: self.id().to_string(),
            items,
        })
    }

    fn cli_commands(&self) -> Vec<CliCommand> {
        vec![CliCommand {
            name: "books",
            about: "Manage book reviews",
            subcommands: vec![
                CliSubcommand {
                    name: "add",
                    about: "Add a book review",
                    args: vec![
                        CliArg {
                            long: "title",
                            short: Some('t'),
                            help: "Book title",
                            required: true,
                        },
                        CliArg {
                            long: "author",
                            short: Some('a'),
                            help: "Book author",
                            required: false,
                        },
                        CliArg {
                            long: "rating",
                            short: Some('r'),
                            help: "Rating (1-10)",
                            required: false,
                        },
                    ],
                    handler: Some(Arc::new(BookAddHandler)),
                },
                CliSubcommand {
                    name: "refresh",
                    about: "Re-fetch book metadata (category/publisher/page_count) for an entry or --all",
                    args: vec![
                        CliArg {
                            long: "id",
                            short: Some('i'),
                            help: "Book id to refresh (mutually exclusive with --all)",
                            required: false,
                        },
                        CliArg {
                            long: "all",
                            short: None,
                            help: "Refresh every entry (per-entry failures are logged and skipped)",
                            required: false,
                        },
                    ],
                    handler: Some(Arc::new(BookRefreshHandler)),
                },
            ],
        }]
    }

    fn setup_wizard(&self) -> Option<ExtensionWizard> {
        Some(ExtensionWizard {
            steps: vec![
                SetupStep {
                    id: "books_key",
                    title_ko: "알라딘 API 키",
                    title_en: "Aladin API key",
                    description_ko: "도서 정보 연동을 위한 알라딘 TTBKey (선택)",
                    description_en: "Aladin TTBKey for book data (optional)",
                    fields: vec![SetupField {
                        name: "aladin_key",
                        label_ko: "알라딘 TTBKey",
                        label_en: "Aladin TTBKey",
                        kind: SetupFieldKind::Secret,
                        required: false,
                        placeholder_ko: None,
                        placeholder_en: None,
                    }],
                    save_handler: Arc::new(BooksKeySave),
                    prefill: BTreeMap::new(),
                    visible_when: None,
                },
                SetupStep {
                    id: "books_test",
                    title_ko: "도서 API 연결 테스트",
                    title_en: "Book API connection test",
                    description_ko: "입력한 키로 도서 검색이 되는지 확인합니다",
                    description_en: "Verify book search works with the key",
                    fields: vec![],
                    save_handler: Arc::new(BooksTestSave),
                    prefill: BTreeMap::new(),
                    visible_when: Some(VisibilityRule::FieldNotEmpty {
                        step_id: "books_key",
                        field: "aladin_key",
                    }),
                },
            ],
        })
    }
}

struct BooksTestSave;
#[async_trait]
impl SetupSaveHandler for BooksTestSave {
    async fn save(
        &self,
        _ctx: &AppState,
        _form: &serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<StepOutcome> {
        let client = client::BooksClient::from_env();
        let ok = client.search("test", 1).await.is_ok();
        let mut m = serde_json::Map::new();
        m.insert(
            "connection_ok".into(),
            if ok { "true" } else { "false" }.into(),
        );
        Ok(StepOutcome { values: m })
    }
}

struct BooksKeySave;
#[async_trait]
impl SetupSaveHandler for BooksKeySave {
    async fn save(
        &self,
        ctx: &AppState,
        form: &serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<StepOutcome> {
        if let Some(v) = form.get("aladin_key").and_then(|x| x.as_str())
            && !v.is_empty()
        {
            // SAFETY: setup wizard 는 단일 사용자 로컬 환경에서만 동작.
            unsafe {
                std::env::set_var("OXIBUILDER_ALADIN_TTBKEY", v);
            }
            persist_extension_config(ctx, "books", "OXIBUILDER_ALADIN_TTBKEY", v).await?;
        }
        Ok(StepOutcome::from_form(form))
    }
}

impl BuildExt for BooksExtension {
    fn ext_id(&self) -> &'static str {
        "books"
    }

    fn build_pages(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<StaticPage>, Box<dyn Error + Send + Sync>> {
        let books: Vec<model::Book> = rt.block_on(repo::list(db, None, 1000, false))?;
        let mut pages = Vec::with_capacity(books.len());
        for b in &books {
            let excerpt: String = b
                .review_ko
                .as_deref()
                .or(b.review_en.as_deref())
                .unwrap_or("")
                .chars()
                .take(160)
                .collect();
            pages.push(StaticPage {
                path: format!("books/{}/index.html", b.id),
                content: format!(
                    r#"<!DOCTYPE html><html lang="ko"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1.0">
    <title>{title}</title><meta property="og:title" content="{title}"><meta property="og:description" content="{excerpt}">
    <meta property="og:type" content="website"><meta property="og:url" content="/books/{id}/">
    <link rel="canonical" href="/books/{id}/"></head><body><div id="root"></div><script src="/assets/index.js"></script></body></html>"#,
                    title=b.title, excerpt=excerpt, id=b.id),
            });
        }
        Ok(pages)
    }

    fn build_data(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn Error + Send + Sync>> {
        let books: Vec<model::Book> = rt.block_on(repo::list(db, None, 1000, false))?;
        Ok(Box::new(books))
    }

    fn build_search_docs(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<SearchDoc>, Box<dyn Error + Send + Sync>> {
        let books: Vec<model::Book> = rt.block_on(repo::list(db, None, 1000, false))?;
        Ok(books
            .into_iter()
            .map(|b| {
                let body = b.review_ko.or(b.review_en).unwrap_or_default();
                let excerpt: String = body.chars().take(200).collect();
                SearchDoc {
                    id: format!("books/{}", b.id),
                    title: b.title,
                    body_preview: excerpt,
                    doc_type: "books".into(),
                    url: format!("/books/{}", b.id),
                    published_at: b.finished_at,
                }
            })
            .collect())
    }

    fn external_image_urls(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        // `cover_image_url` is already a full URL (Aladin or Google Books API),
        // stored verbatim by `client.rs`. Filter to http(s) and dedupe-empty.
        let rows: Vec<(Option<String>,)> = rt.block_on(async {
            sqlx::query_as(
                "SELECT cover_image_url FROM book_entry \
                 WHERE cover_image_url LIKE 'http%'",
            )
            .fetch_all(db)
            .await
        })?;
        Ok(rows.into_iter().filter_map(|(u,)| u).collect())
    }
}
