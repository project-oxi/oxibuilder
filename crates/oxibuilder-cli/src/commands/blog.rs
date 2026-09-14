use crate::output::Output;
use clap::Subcommand;
use oxibuilder_ext_blog::model::{BlogPatch, BlogPostInput, BlogSeriesInput};
use oxibuilder_ext_blog::repo;

#[derive(Subcommand, Debug, Clone)]
pub enum BlogCommand {
    New {
        title: String,
        #[arg(long, default_value = "ko")]
        lang: String,
        #[arg(long, help = "본문 마크다운 파일. 미지정 시 빈 본문")]
        file: Option<std::path::PathBuf>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(
            long = "translation-of",
            help = "이 글이 번역원인 글의 slug — 같은 translation group으로 묶임"
        )]
        translation_of: Option<String>,
        #[arg(long, help = "카테고리 (자유 텍스트)")]
        category: Option<String>,
        #[arg(long, help = "시리즈 slug — 사전에 'blog series new'로 생성")]
        series: Option<String>,
        #[arg(long, help = "시리즈 내 순서 (화번호)")]
        order: Option<i64>,
        #[arg(long, help = "즉시 발행 (초안 우선 원칙 위반 — 명시적 승인)")]
        publish: bool,
    },
    /// 초안 발행 (별도 승인 단계).
    Publish { slug: String },
    /// 목록 (기본: 발행본만. --draft로 초안만).
    List {
        #[arg(long)]
        draft: bool,
        #[arg(long)]
        lang: Option<String>,
    },
    /// 단건 조회.
    Show { slug: String },
    /// 수정 (title/body/tags/category/series).
    Edit {
        slug: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, help = "본문 마크다운 파일")]
        file: Option<std::path::PathBuf>,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, help = "카테고리 변경. 빈 문자열이면 해제")]
        category: Option<String>,
        #[arg(long, help = "시리즈 변경/지정")]
        series: Option<String>,
        #[arg(long, help = "시리즈 해제")]
        clear_series: bool,
        #[arg(long, help = "시리즈 순서 변경")]
        order: Option<i64>,
    },
    /// 삭제.
    Rm { slug: String },
    /// 시리즈 관리.
    Series {
        #[command(subcommand)]
        cmd: SeriesCommand,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SeriesCommand {
    /// 새 시리즈 생성.
    New {
        title: String,
        #[arg(long, help = "시리즈 설명")]
        desc: Option<String>,
        #[arg(long, help = "slug 미지정 시 title에서 자동 생성")]
        slug: Option<String>,
    },
    /// 시리즈 목록.
    List,
    /// 시리즈 상세 (발행 글 포함).
    Show { slug: String },
}

pub(crate) async fn blog(c: BlogCommand, out: &Output) -> anyhow::Result<()> {
    let data_dir = super::resolve_data_dir()?;
    let pool = oxibuilder_core::db::connect(&data_dir.join("oxibuilder.db")).await?;

    match c {
        BlogCommand::New {
            title,
            lang,
            file,
            tags,
            translation_of,
            category,
            series,
            order,
            publish,
        } => {
            let body = match file {
                Some(p) => std::fs::read_to_string(p)?,
                None => String::new(),
            };
            let translation_group_id = match &translation_of {
                Some(of) => {
                    let target = repo::find_by_slug(&pool, of)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("translation source not found: {of}"))?;
                    if target.lang == lang {
                        anyhow::bail!(
                            "translation source '{of}' is already '{lang}'; translations must differ in lang"
                        );
                    }
                    Some(repo::translation_group_for(&pool, of).await?)
                }
                None => None,
            };
            let input = BlogPostInput {
                title,
                body,
                lang,
                tags,
                translation_group_id,
                slug: None,
                category,
                series,
                series_order: order,
            };
            let slug_base = repo::slugify(&input.title);
            let resolved_slug = repo::ensure_unique_slug(&pool, &slug_base).await?;
            let post = repo::create(&pool, &input, &resolved_slug).await?;
            if publish {
                let published = repo::publish(&pool, &post.slug).await?;
                out.data(serde_json::to_value(&published)?, "published")
            } else {
                out.data(serde_json::to_value(&post)?, "draft created")
            }
        }
        BlogCommand::Publish { slug } => {
            let post = repo::publish(&pool, &slug).await?;
            out.data(serde_json::to_value(&post)?, "published")
        }
        BlogCommand::List { draft, lang } => {
            let posts = repo::list(&pool, draft, lang.as_deref(), 200).await?;
            out.data(serde_json::to_value(&posts)?, "posts")
        }
        BlogCommand::Show { slug } => {
            let post = repo::find_by_slug(&pool, &slug)
                .await?
                .ok_or_else(|| anyhow::anyhow!("post not found: {slug}"))?;
            out.data(serde_json::to_value(&post)?, "post")
        }
        BlogCommand::Edit {
            slug,
            title,
            file,
            tags,
            category,
            series,
            clear_series,
            order,
        } => {
            let body = match file {
                Some(p) => Some(std::fs::read_to_string(p)?),
                None => None,
            };
            let tags_opt = if tags.is_empty() { None } else { Some(tags) };
            // 빈 문자열 카테고리 = 해제(NULL).
            let category_patch = category.map(|c| if c.is_empty() { None } else { Some(c) });
            let series_patch = if clear_series {
                Some(None)
            } else {
                series.map(Some)
            };
            let patch = BlogPatch {
                title,
                body,
                lang: None,
                tags: tags_opt,
                category: category_patch,
                series: series_patch,
                series_order: order.map(Some),
            };
            let post = repo::update(&pool, &slug, &patch)
                .await?
                .ok_or_else(|| anyhow::anyhow!("post not found: {slug}"))?;
            out.data(serde_json::to_value(&post)?, "updated")
        }
        BlogCommand::Rm { slug } => {
            let removed = repo::delete(&pool, &slug).await?;
            if removed {
                out.ok(format!("deleted: {slug}"))
            } else {
                anyhow::bail!("post not found: {slug}")
            }
        }
        BlogCommand::Series { cmd } => match cmd {
            SeriesCommand::New { title, desc, slug } => {
                let input = BlogSeriesInput {
                    title,
                    description: desc.unwrap_or_default(),
                    slug,
                };
                let s = repo::series_create(&pool, &input).await?;
                out.data(serde_json::to_value(&s)?, "series created")
            }
            SeriesCommand::List => {
                let all = repo::series_list(&pool).await?;
                out.data(serde_json::to_value(&all)?, "series")
            }
            SeriesCommand::Show { slug } => {
                let s = repo::series_find_by_slug(&pool, &slug)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("series not found: {slug}"))?;
                let posts = repo::series_posts(&pool, s.id).await?;
                out.data(
                    serde_json::json!({
                        "series": s,
                        "posts": posts,
                    }),
                    "series detail",
                )
            }
        },
    }
}
