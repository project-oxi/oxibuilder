use crate::model::{BlogPatch, BlogPost, BlogSeries, BlogSeriesInput, BlogPostInput};
use sqlx::SqlitePool;

const COLUMNS: &str = "id, slug, title, body, lang, translation_group_id, tags,
                       published_at, created_at, updated_at,
                       category, series_id, series_order";

/// title 기반 slug 자동 생성.
/// 영문/숫자는 그대로 소문자, 공백/구두점은 '-', 한글 등은 그대로 (URL에 percent-encoded).
/// 결과가 빈 문자열이면 `post-<unix_ts>` 폴백.
pub fn slugify(title: &str) -> String {
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = base.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("post-{}", unix_ts())
    } else {
        trimmed
    }
}

fn unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// slug 충돌 회피: 이미 존재하면 -2, -3, ... suffix.
pub async fn ensure_unique_slug(pool: &SqlitePool, base: &str) -> anyhow::Result<String> {
    if !slug_exists(pool, base).await? {
        return Ok(base.to_string());
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !slug_exists(pool, &candidate).await? {
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not allocate unique slug for {base}")
}

async fn slug_exists(pool: &SqlitePool, slug: &str) -> anyhow::Result<bool> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM blog_post WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await?;
    Ok(row.0 > 0)
}

pub async fn create(
    pool: &SqlitePool,
    input: &BlogPostInput,
    resolved_slug: &str,
) -> anyhow::Result<BlogPost> {
    let tags = serde_json::to_string(&input.tags)?;
    let series_id = match &input.series {
        Some(slug) => Some(resolve_series_id(pool, slug).await?),
        None => None,
    };
    let post = sqlx::query_as::<_, BlogPost>(&format!(
        "INSERT INTO blog_post (slug, title, body, lang, translation_group_id, tags, category, series_id, series_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         RETURNING {COLUMNS}"
    ))
    .bind(resolved_slug)
    .bind(&input.title)
    .bind(&input.body)
    .bind(&input.lang)
    .bind(input.translation_group_id)
    .bind(tags)
    .bind(&input.category)
    .bind(series_id)
    .bind(input.series_order)
    .fetch_one(pool)
    .await?;
    Ok(post)
}

pub async fn find_by_slug(pool: &SqlitePool, slug: &str) -> anyhow::Result<Option<BlogPost>> {
    let post =
        sqlx::query_as::<_, BlogPost>(&format!("SELECT {COLUMNS} FROM blog_post WHERE slug = ?"))
            .bind(slug)
            .fetch_optional(pool)
            .await?;
    Ok(post)
}

/// draft=true → 초안(published_at IS NULL)만.
/// draft=false → 발행본(published_at NOT NULL)만.
pub async fn list(
    pool: &SqlitePool,
    draft: bool,
    lang: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<BlogPost>> {
    let limit = limit.clamp(1, 200);
    let (sql, has_lang) = match (draft, lang.is_some()) {
        (true, true) => (
            format!(
                "SELECT {COLUMNS} FROM blog_post
                 WHERE published_at IS NULL AND lang = ?
                 ORDER BY created_at DESC LIMIT ?"
            ),
            true,
        ),
        (true, false) => (
            format!(
                "SELECT {COLUMNS} FROM blog_post
                 WHERE published_at IS NULL
                 ORDER BY created_at DESC LIMIT ?"
            ),
            false,
        ),
        (false, true) => (
            format!(
                "SELECT {COLUMNS} FROM blog_post
                 WHERE published_at IS NOT NULL AND lang = ?
                 ORDER BY published_at DESC LIMIT ?"
            ),
            true,
        ),
        (false, false) => (
            format!(
                "SELECT {COLUMNS} FROM blog_post
                 WHERE published_at IS NOT NULL
                 ORDER BY published_at DESC LIMIT ?"
            ),
            false,
        ),
    };
    let mut q = sqlx::query_as::<_, BlogPost>(&sql);
    if has_lang && let Some(l) = lang {
        q = q.bind(l);
    }
    let posts = q.bind(limit).fetch_all(pool).await?;
    Ok(posts)
}

pub async fn publish(pool: &SqlitePool, slug: &str) -> anyhow::Result<BlogPost> {
    let post = sqlx::query_as::<_, BlogPost>(&format!(
        "UPDATE blog_post
            SET published_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
          WHERE slug = ?
         RETURNING {COLUMNS}"
    ))
    .bind(slug)
    .fetch_one(pool)
    .await?;
    Ok(post)
}

pub async fn update(
    pool: &SqlitePool,
    slug: &str,
    patch: &BlogPatch,
) -> anyhow::Result<Option<BlogPost>> {
    // 부분 갱신 — 제공된 필드만. 이중 Option 필드는 바깥 Some일 때만 SET한다.
    // 바인드는 전부 위치 기반 `?` (sqlx-sqlite는 `?name` 형태를 지원하지 않는다).
    let mut sets: Vec<&str> = Vec::new();
    if patch.title.is_some() {
        sets.push("title = ?");
    }
    if patch.body.is_some() {
        sets.push("body = ?");
    }
    if patch.lang.is_some() {
        sets.push("lang = ?");
    }
    if patch.tags.is_some() {
        sets.push("tags = ?");
    }
    if patch.category.is_some() {
        sets.push("category = ?");
    }
    if patch.series.is_some() {
        sets.push("series_id = ?");
        // series_order를 함께 지정하지 않으면 기존 order를 보존한다.
        if patch.series_order.is_none() {
            sets.push("series_order = COALESCE(?, series_order)");
        }
    }
    if patch.series_order.is_some() {
        sets.push("series_order = ?");
    }
    if sets.is_empty() {
        return find_by_slug(pool, slug).await;
    }
    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')");
    let set_clause = sets.join(", ");
    let tags_json = match &patch.tags {
        Some(t) => Some(serde_json::to_string(t)?),
        None => None,
    };
    // series가 지정되면 slug → id 해소 (Some(None)은 NULL 바인드로 해제).
    let series_id = match &patch.series {
        Some(Some(series_slug)) => Some(resolve_series_id(pool, series_slug).await?),
        _ => None,
    };
    let query_str =
        format!("UPDATE blog_post SET {set_clause} WHERE slug = ? RETURNING {COLUMNS}");
    let mut q = sqlx::query_as::<_, BlogPost>(&query_str);
    if let Some(v) = &patch.title {
        q = q.bind(v);
    }
    if let Some(v) = &patch.body {
        q = q.bind(v);
    }
    if let Some(v) = &patch.lang {
        q = q.bind(v);
    }
    if let Some(v) = tags_json.as_ref() {
        q = q.bind(v);
    }
    if patch.category.is_some() {
        q = q.bind(patch.category.clone().flatten());
    }
    if patch.series.is_some() {
        q = q.bind(series_id);
    }
    if patch.series_order.is_some() || patch.series.is_some() {
        q = q.bind(patch.series_order.clone().flatten());
    }
    let post = q.bind(slug).fetch_optional(pool).await?;
    Ok(post)
}

pub async fn delete(pool: &SqlitePool, slug: &str) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM blog_post WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// translation_group에 속한 글(자신 제외).
pub async fn list_translations(
    pool: &SqlitePool,
    group_id: i64,
    exclude_slug: &str,
) -> anyhow::Result<Vec<BlogPost>> {
    let posts = sqlx::query_as::<_, BlogPost>(&format!(
        "SELECT {COLUMNS} FROM blog_post
         WHERE translation_group_id = ? AND slug != ?
         ORDER BY lang"
    ))
    .bind(group_id)
    .bind(exclude_slug)
    .fetch_all(pool)
    .await?;
    Ok(posts)
}

/// `--translation-of`로 지정한 원본 글의 translation group을 확정한다.
/// 원본이 이미 그룹에 속하면 그 id를 재사용하고, 아니면 원본 자신의 id를
/// 그룹으로 승격시켜 반환한다 (그룹 id = 그룹 원본 글의 id).
pub async fn translation_group_for(pool: &SqlitePool, target_slug: &str) -> anyhow::Result<i64> {
    let target = find_by_slug(pool, target_slug)
        .await?
        .ok_or_else(|| anyhow::anyhow!("post not found: {target_slug}"))?;
    if let Some(group) = target.translation_group_id {
        return Ok(group);
    }
    sqlx::query("UPDATE blog_post SET translation_group_id = ?1 WHERE slug = ?2")
        .bind(target.id)
        .bind(target_slug)
        .execute(pool)
        .await?;
    Ok(target.id)
}

// ──────────────────── 시리즈 ────────────────────

const SERIES_COLUMNS: &str = "id, slug, title, description, created_at, updated_at";

pub async fn series_create(pool: &SqlitePool, input: &BlogSeriesInput) -> anyhow::Result<BlogSeries> {
    let base = input.slug.clone().unwrap_or_else(|| slugify(&input.title));
    let slug = ensure_unique_series_slug(pool, &base).await?;
    let series = sqlx::query_as::<_, BlogSeries>(&format!(
        "INSERT INTO blog_series (slug, title, description)
         VALUES (?1, ?2, ?3)
         RETURNING {SERIES_COLUMNS}"
    ))
    .bind(&slug)
    .bind(&input.title)
    .bind(&input.description)
    .fetch_one(pool)
    .await?;
    Ok(series)
}

pub async fn series_list(pool: &SqlitePool) -> anyhow::Result<Vec<BlogSeries>> {
    let series = sqlx::query_as::<_, BlogSeries>(&format!(
        "SELECT {SERIES_COLUMNS} FROM blog_series ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(series)
}

pub async fn series_find_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> anyhow::Result<Option<BlogSeries>> {
    let series = sqlx::query_as::<_, BlogSeries>(&format!(
        "SELECT {SERIES_COLUMNS} FROM blog_series WHERE slug = ?"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(series)
}

/// 시리즈의 발행된 글을 series_order 순(미지정은 뒤로, 같으면 발행 시간순)으로.
pub async fn series_posts(pool: &SqlitePool, series_id: i64) -> anyhow::Result<Vec<BlogPost>> {
    let posts = sqlx::query_as::<_, BlogPost>(&format!(
        "SELECT {COLUMNS} FROM blog_post
         WHERE series_id = ? AND published_at IS NOT NULL
         ORDER BY COALESCE(series_order, 9223372036854775807), published_at"
    ))
    .bind(series_id)
    .fetch_all(pool)
    .await?;
    Ok(posts)
}

/// series slug → id. 없으면 에러 (호출부가 422/CLI 에러로 변환).
pub async fn resolve_series_id(pool: &SqlitePool, slug: &str) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT id FROM blog_series WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("series not found: {slug}"))?;
    Ok(row.0)
}

async fn ensure_unique_series_slug(pool: &SqlitePool, base: &str) -> anyhow::Result<String> {
    let exists = |row: (i64,)| row.0 > 0;
    let count = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM blog_series WHERE slug = ?")
        .bind(base)
        .fetch_one(pool)
        .await?;
    if !exists(count) {
        return Ok(base.to_string());
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        let count = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM blog_series WHERE slug = ?")
            .bind(&candidate)
            .fetch_one(pool)
            .await?;
        if !exists(count) {
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not allocate unique series slug for {base}")
}
