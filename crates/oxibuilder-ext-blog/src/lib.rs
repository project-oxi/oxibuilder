pub mod model;
pub mod repo;
pub mod routes;

use async_trait::async_trait;
use axum::Router;
use axum::routing::{get, post};
use oxibuilder_core::builder::{BuildExt, SearchDoc, StaticPage};
use oxibuilder_core::extension::{Extension, Lang, LobbyCard, LobbyCardItem, Migration};
use oxibuilder_core::state::AppState;
use sqlx::SqlitePool;
use std::error::Error;

/// 환영 글 slug. 이미 같은 slug가 있으면 시드하지 않음 (멱등성).
const WELCOME_POST_SLUG: &str = "환영합니다";

/// 환영 글 본문 (markdown). 확장이 자기 도메인 데이터를 정의한다 — 코어는 모른다.
const WELCOME_POST_BODY: &str = r#"# 환영합니다!

Oxibuilder 설치가 완료되었습니다.

이 글은 설정 마법사가 생성한 샘플 글입니다.
삭제하거나 수정해도 됩니다.

## 다음 단계

- **CLI**로 글 쓰기: `oxibuilder blog new "제목" --file draft.md`
- **관리 콘솔**에서 콘텐츠 관리: 헤더의 설정 버튼
- **프로젝트** 추가: `oxibuilder project add --title-ko "..." --title-en "..."`

즐거운 블로그 생활 되세요!
"#;

pub struct BlogExtension {
    /// Build-time image manifest; populated by the build command before
    /// `build_pages` runs (Task 5) so post bodies can emit responsive
    /// `<img srcset>` for `media/...` refs. `None` during tests / dev.
    pub manifest: std::sync::OnceLock<oxibuilder_core::media::ImageManifest>,
}

impl BlogExtension {
    pub fn new() -> Self {
        Self {
            manifest: std::sync::OnceLock::new(),
        }
    }

    /// Idempotent — first call wins, later calls are silently ignored.
    pub fn set_manifest(&self, m: oxibuilder_core::media::ImageManifest) {
        let _ = self.manifest.set(m);
    }
}

impl Default for BlogExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Extension for BlogExtension {
    fn id(&self) -> &'static str {
        "blog"
    }
    fn table_names(&self) -> Vec<&'static str> {
        vec!["blog_post"]
    }

    fn display_name(&self, lang: Lang) -> String {
        match lang {
            Lang::Ko => "블로그".to_string(),
            Lang::En => "Blog".to_string(),
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
                name: "series_category",
                sql: include_str!("../migrations/0002_series_category.sql"),
            },
        ]
    }

    fn routes(&self) -> Router {
        Router::new()
            .route("/", get(routes::list).post(routes::create))
            .route(
                "/{slug}",
                get(routes::show)
                    .patch(routes::update)
                    .delete(routes::delete),
            )
            .route("/{slug}/publish", post(routes::publish))
            .route(
                "/series",
                get(routes::series_list).post(routes::series_create),
            )
            .route("/series/{slug}", get(routes::series_show))
    }
    async fn seed_sample_data(&self, ctx: &AppState) -> anyhow::Result<()> {
        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM blog_post WHERE slug = ?1")
            .bind(WELCOME_POST_SLUG)
            .fetch_one(&ctx.db)
            .await?;
        if exists.0 > 0 {
            return Ok(()); // 멱등성 — 이미 있으면 시드 안 함
        }
        sqlx::query(
            "INSERT INTO blog_post (slug, title, body, lang, tags, published_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'ko', '[]',
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        )
        .bind(WELCOME_POST_SLUG)
        .bind(WELCOME_POST_SLUG) // title도 동일
        .bind(WELCOME_POST_BODY)
        .execute(&ctx.db)
        .await?;
        Ok(())
    }

    async fn lobby_summary(&self, ctx: &AppState) -> Option<LobbyCard> {
        let posts = repo::list(&ctx.db, false, None, 3).await.ok()?;
        let items = posts
            .into_iter()
            .map(|p| LobbyCardItem {
                title: p.title,
                url: format!("/blog/{}", p.slug),
            })
            .collect();
        Some(LobbyCard {
            id: self.id().to_string(),
            items,
        })
    }
}

impl BuildExt for BlogExtension {
    fn ext_id(&self) -> &'static str {
        "blog"
    }

    fn build_pages(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<StaticPage>, Box<dyn Error + Send + Sync>> {
        let posts: Vec<model::BlogPost> = rt.block_on(repo::list(db, false, None, i64::MAX))?;

        let mut pages = Vec::with_capacity(posts.len() * 3);

        // Snapshot the manifest once — set by the build command before
        // `build_pages` runs (Task 5). Missing in tests / dev is fine;
        // `markdown::render` just falls back to plain `<img>` tags.
        let images = self.manifest.get().cloned().unwrap_or_default();

        for post in &posts {
            // HTML page with OG metas
            let excerpt = body_excerpt(&post.body, 160);
            // Render markdown → HTML with BASE_PLACEHOLDER as asset base;
            // Task 5 substitutes the real `deployment_base` (e.g. `/blog/`)
            // into the emitted HTML after the build writes the page.
            let body_html = oxibuilder_core::markdown::render(
                &post.body,
                oxibuilder_core::markdown::BASE_PLACEHOLDER,
                &images,
            );

            // Absolute OG image URL — resolved to `{origin}{deployment_base}oxibuilder.png`
            // by build_writer's OG_IMAGE_PLACEHOLDER substitution (crawlers reject
            // relative og:image values).
            let og_image = format!(
                "{}oxibuilder.png",
                oxibuilder_core::markdown::OG_IMAGE_PLACEHOLDER
            );

            let html = format!(
                r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <meta property="og:title" content="{title}">
  <meta property="og:description" content="{excerpt}">
  <meta property="og:type" content="article">
  <meta property="og:url" content="/blog/{slug}/">
  <meta property="og:image" content="{og_image}">
  <link rel="icon" type="image/png" href="favicon-32.png">
  <meta name="twitter:card" content="summary">
  <link rel="canonical" href="/blog/{slug}/">
</head>
<body>
  <div id="root"><article class="markdown">{body}</article></div>
  <script src="/assets/index.js"></script>
</body>
</html>
"#,
                lang = post.lang,
                title = post.title,
                slug = post.slug,
                excerpt = excerpt,
                og_image = og_image,
                body = body_html,
            );

            pages.push(StaticPage {
                path: format!("blog/{}/index.html", post.slug),
                content: html,
            });

            // Markdown source file
            pages.push(StaticPage {
                path: format!("blog/{}/index.md", post.slug),
                content: post.body.clone(),
            });

            // JSON metadata
            let meta = serde_json::json!({
                "title": post.title,
                "slug": post.slug,
                "lang": post.lang,
                "tags": post.tags,
                "published_at": post.published_at,
            });
            pages.push(StaticPage {
                path: format!("blog/{}/index.json", post.slug),
                content: serde_json::to_string_pretty(&meta).unwrap_or_default(),
            });
        }

        // 시리즈 페이지 — 발행 글이 1개 이상인 시리즈만. no-JS 폴백 목록을
        // 인라인하고 SPA는 /blog/series/{slug} 라우트에서 하이드레이션한다.
        let series: Vec<model::BlogSeries> = rt.block_on(repo::series_list(db))?;
        for s in &series {
            let series_posts: Vec<model::BlogPost> =
                rt.block_on(repo::series_posts(db, s.id))?;
            if series_posts.is_empty() {
                continue;
            }
            let items: String = series_posts
                .iter()
                .map(|p| {
                    let date = p.published_at.as_deref().and_then(|s| s.get(..10)).unwrap_or("");
                    format!(
                        r#"<li><a href="/blog/{slug}/">{title}</a> <time>{date}</time></li>"#,
                        slug = html_escape(&p.slug),
                        title = html_escape(&p.title),
                        date = date,
                    )
                })
                .collect();
            let html = format!(
                r#"<!DOCTYPE html>
<html lang="{lang}">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <meta property="og:title" content="{title}">
  <meta property="og:description" content="{excerpt}">
  <meta property="og:type" content="website">
  <meta property="og:url" content="/blog/series/{slug}/">
  <link rel="icon" type="image/png" href="favicon-32.png">
  <link rel="canonical" href="/blog/series/{slug}/">
</head>
<body>
  <div id="root"><section class="markdown"><h1>{title}</h1><p>{excerpt}</p><ol>{items}</ol></section></div>
  <script src="/assets/index.js"></script>
</body>
</html>
"#,
                lang = series_posts[0].lang,
                title = html_escape(&s.title),
                excerpt = html_escape(&s.description),
                slug = html_escape(&s.slug),
                items = items,
            );
            pages.push(StaticPage {
                path: format!("blog/series/{}/index.html", s.slug),
                content: html,
            });
        }

        Ok(pages)
    }

    fn build_data(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn Error + Send + Sync>> {
        let posts: Vec<model::BlogPost> = rt.block_on(repo::list(db, false, None, i64::MAX))?;
        let series: Vec<model::BlogSeries> = rt.block_on(repo::series_list(db))?;
        Ok(Box::new(serde_json::json!({
            "posts": posts,
            "series": series,
        })))
    }

    fn build_search_docs(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<SearchDoc>, Box<dyn Error + Send + Sync>> {
        let posts: Vec<model::BlogPost> = rt.block_on(repo::list(db, false, None, i64::MAX))?;

        let docs: Vec<SearchDoc> = posts
            .into_iter()
            .map(|p| {
                let excerpt = body_excerpt(&p.body, 200);
                SearchDoc {
                    id: format!("blog/{}", p.slug),
                    title: p.title,
                    body_preview: excerpt,
                    doc_type: "blog".to_string(),
                    url: format!("/blog/{}", p.slug),
                    published_at: p.published_at,
                }
            })
            .collect();

        Ok(docs)
    }
}
/// Body excerpt for search index / OG description.
fn body_excerpt(body: &str, max_chars: usize) -> String {
    let plain: String = body.chars().filter(|c| !c.is_control()).collect();
    let excerpt: String = plain.chars().take(max_chars).collect();
    if excerpt.len() < plain.len() {
        format!("{}…", excerpt)
    } else {
        excerpt
    }
}

/// Minimal HTML text escaping for slugs/titles interpolated into static pages.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
