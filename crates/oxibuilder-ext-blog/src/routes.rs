use crate::model::{BlogPatch, BlogPost, BlogSeriesDetail, BlogSeriesInput, BlogPostInput, BlogSeries, ListQuery};
use crate::repo;
use axum::Json;
use axum::extract::{Extension, Path, Query};
use oxibuilder_core::error::ApiError;
use oxibuilder_core::extension::DataEnvelope;
use oxibuilder_core::search;
use oxibuilder_core::state::SiteScopedDb;
use sqlx::SqlitePool;

pub async fn list(
    Extension(pool): Extension<SiteScopedDb>,
    Query(q): Query<ListQuery>,
) -> Result<Json<DataEnvelope<Vec<BlogPost>>>, ApiError> {
    let limit = q.limit.unwrap_or(20);
    let posts = repo::list(&pool.db, q.draft, q.lang.as_deref(), limit)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope { data: posts }))
}

pub async fn create(
    Extension(pool): Extension<SiteScopedDb>,
    Json(input): Json<BlogPostInput>,
) -> Result<Json<DataEnvelope<BlogPost>>, ApiError> {
    // Server-authoritative validation: title required, lang must be enabled
    // for the site (reads the live settings snapshot, not a hardcoded list).
    if input.title.trim().is_empty() {
        return Err(ApiError::validation("title", "title must not be empty"));
    }
    let enabled: std::collections::BTreeSet<String> = pool
        .settings
        .read()
        .await
        .site
        .languages
        .iter()
        .cloned()
        .collect();
    if !enabled.contains(&input.lang) {
        return Err(ApiError::validation(
            "lang",
            "lang is not enabled for this site",
        ));
    }
    validate_input(&input)?;
    if let Some(series) = &input.series
        && repo::series_find_by_slug(&pool.db, series)
            .await
            .map_err(ApiError::internal)?
            .is_none()
    {
        return Err(ApiError::validation("series", "series not found"));
    }
    let base_slug = input
        .slug
        .clone()
        .unwrap_or_else(|| repo::slugify(&input.title));
    let slug = repo::ensure_unique_slug(&pool.db, &base_slug)
        .await
        .map_err(ApiError::internal)?;
    let post = repo::create(&pool.db, &input, &slug)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope { data: post }))
}

pub async fn show(
    Extension(pool): Extension<SiteScopedDb>,
    Path(slug): Path<String>,
) -> Result<Json<DataEnvelope<BlogPost>>, ApiError> {
    let post = repo::find_by_slug(&pool.db, &slug)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| not_found(&slug))?;
    if post.published_at.is_none() {
        return Err(not_found(&slug));
    }
    Ok(Json(DataEnvelope { data: post }))
}

pub async fn update(
    Extension(pool): Extension<SiteScopedDb>,
    Path(slug): Path<String>,
    Json(patch): Json<BlogPatch>,
) -> Result<Json<DataEnvelope<BlogPost>>, ApiError> {
    if let Some(lang) = &patch.lang
        && lang != "ko"
        && lang != "en"
    {
        return Err(ApiError::validation("lang", "lang must be 'ko' or 'en'"));
    }
    let post = repo::update(&pool.db, &slug, &patch)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| not_found(&slug))?;
    if post.published_at.is_some() {
        reindex(&pool.db, &post).await?;
    }
    Ok(Json(DataEnvelope { data: post }))
}

pub async fn delete(
    Extension(pool): Extension<SiteScopedDb>,
    Path(slug): Path<String>,
) -> Result<Json<DataEnvelope<serde_json::Value>>, ApiError> {
    let removed = repo::delete(&pool.db, &slug)
        .await
        .map_err(ApiError::internal)?;
    if !removed {
        return Err(not_found(&slug));
    }
    search::delete(&pool.db, "blog", &slug)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope {
        data: serde_json::json!({ "slug": slug, "deleted": true }),
    }))
}

pub async fn publish(
    Extension(pool): Extension<SiteScopedDb>,
    Path(slug): Path<String>,
) -> Result<Json<DataEnvelope<BlogPost>>, ApiError> {
    if repo::find_by_slug(&pool.db, &slug)
        .await
        .map_err(ApiError::internal)?
        .is_none()
    {
        return Err(not_found(&slug));
    }
    let post = repo::publish(&pool.db, &slug)
        .await
        .map_err(ApiError::internal)?;
    reindex(&pool.db, &post).await?;
    Ok(Json(DataEnvelope { data: post }))
}

async fn reindex(db: &SqlitePool, post: &BlogPost) -> Result<(), ApiError> {
    search::upsert(
        db,
        "blog",
        &post.slug,
        &post.title,
        &post.body,
        Some(&post.lang),
        post.published_at.as_deref(),
    )
    .await
    .map_err(ApiError::internal)
}

fn validate_input(input: &BlogPostInput) -> Result<(), ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::validation("title", "title must not be empty"));
    }
    if input.lang != "ko" && input.lang != "en" {
        return Err(ApiError::validation("lang", "lang must be 'ko' or 'en'"));
    }
    Ok(())
}

fn not_found(slug: &str) -> ApiError {
    ApiError::new(
        axum::http::StatusCode::NOT_FOUND,
        "not_found",
        &format!("blog post '{slug}' not found"),
    )
}

// ──────────────────── 시리즈 ────────────────────

pub async fn series_list(
    Extension(pool): Extension<SiteScopedDb>,
) -> Result<Json<DataEnvelope<Vec<BlogSeries>>>, ApiError> {
    let series = repo::series_list(&pool.db)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope { data: series }))
}

pub async fn series_create(
    Extension(pool): Extension<SiteScopedDb>,
    Json(input): Json<BlogSeriesInput>,
) -> Result<Json<DataEnvelope<BlogSeries>>, ApiError> {
    if input.title.trim().is_empty() {
        return Err(ApiError::validation("title", "title must not be empty"));
    }
    let series = repo::series_create(&pool.db, &input)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope { data: series }))
}

pub async fn series_show(
    Extension(pool): Extension<SiteScopedDb>,
    Path(slug): Path<String>,
) -> Result<Json<DataEnvelope<BlogSeriesDetail>>, ApiError> {
    let series = repo::series_find_by_slug(&pool.db, &slug)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| not_found(&slug))?;
    let posts = repo::series_posts(&pool.db, series.id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(DataEnvelope {
        data: BlogSeriesDetail { series, posts },
    }))
}
