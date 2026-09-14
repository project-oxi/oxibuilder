use crate::model::{
    GenreInput, GenreName, MovieEntry, MovieEntryDetail, MovieEntryInput, MovieEntryPatch,
    PersonInput, PersonSummary, SeriesGroup, SeriesGroupInput, SeriesGroupPatch,
};
use anyhow::anyhow;
use sqlx::SqlitePool;
use std::collections::HashMap;

const ENTRY_COLUMNS: &str = "id, slug, tmdb_id, media_type, title, title_ko, title_en,
                             poster_path, origin, release_year, runtime_min, watched_at,
                             rating, review_ko, review_en, rewatch,
                             series_group_id, series_order, published_at,
                             created_at, updated_at";

const GROUP_COLUMNS: &str = "id, slug, title_ko, title_en, cover_image,
                              group_rating, group_review_ko, group_review_en,
                              created_at, updated_at";

// ─── Slug helpers ───

/// 제목 → slug. 영문/숫자는 그대로, 그 외는 '-'. 양끝 '-' 제거.
/// 한글이면 전부 '-' 가 되니 폴백이 발동된다.
pub fn slugify(title: &str) -> String {
    slugify_with("movie", title)
}

/// 인물명 → slug. 폴백 접두어만 다르다.
pub fn person_slugify(name: &str) -> String {
    slugify_with("person", name)
}

fn slugify_with(fallback_prefix: &str, s: &str) -> String {
    let base: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = base.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("{fallback_prefix}-{}", unix_ts())
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

/// 지정 테이블에 slug 가 있는지. 테이블명은 내부 상수로만 호출되므로 안전하게 보간.
async fn slug_exists(pool: &SqlitePool, table: &str, slug: &str) -> anyhow::Result<bool> {
    let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table} WHERE slug = ?"))
        .bind(slug)
        .fetch_one(pool)
        .await?;
    Ok(row.0 > 0)
}

pub async fn ensure_unique_entry_slug(pool: &SqlitePool, base: &str) -> anyhow::Result<String> {
    ensure_unique_slug(pool, "movie_entry", base).await
}

pub async fn ensure_unique_group_slug(pool: &SqlitePool, base: &str) -> anyhow::Result<String> {
    ensure_unique_slug(pool, "series_group", base).await
}

pub async fn ensure_unique_person_slug(pool: &SqlitePool, base: &str) -> anyhow::Result<String> {
    ensure_unique_slug(pool, "movie_person", base).await
}

async fn ensure_unique_slug(pool: &SqlitePool, table: &str, base: &str) -> anyhow::Result<String> {
    if !slug_exists(pool, table, base).await? {
        return Ok(base.to_string());
    }
    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !slug_exists(pool, table, &candidate).await? {
            return Ok(candidate);
        }
    }
    anyhow::bail!("could not allocate unique slug for {base}")
}

// ─── MovieEntry ───

#[allow(clippy::too_many_arguments)]
pub async fn create_entry(
    pool: &SqlitePool,
    input: &MovieEntryInput,
    resolved_slug: &str,
    tmdb_id: Option<i64>,
    title: String,
    title_ko: Option<String>,
    title_en: Option<String>,
    poster_path: Option<String>,
    release_year: Option<i32>,
    runtime_min: Option<i32>,
    origin: Option<String>,
) -> anyhow::Result<MovieEntry> {
    let rewatch: i8 = if input.rewatch { 1 } else { 0 };
    let entry = sqlx::query_as::<_, MovieEntry>(&format!(
        "INSERT INTO movie_entry
            (slug, tmdb_id, media_type, title, title_ko, title_en, poster_path,
             origin, release_year, runtime_min, watched_at, rating, review_ko, review_en,
             rewatch, series_group_id, series_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(resolved_slug)
    .bind(tmdb_id)
    .bind(&input.media_type)
    .bind(&title)
    .bind(title_ko.as_deref())
    .bind(title_en.as_deref())
    .bind(poster_path.as_deref())
    .bind(origin.as_deref())
    .bind(release_year)
    .bind(runtime_min)
    .bind(input.watched_at.as_deref())
    .bind(input.rating)
    .bind(input.review_ko.as_deref())
    .bind(input.review_en.as_deref())
    .bind(rewatch)
    .bind(input.series_group_id)
    .bind(input.series_order)
    .fetch_one(pool)
    .await?;

    // 장르/출연진/감독 동기화.
    replace_genres(pool, entry.id, input.genres.as_deref().unwrap_or_default()).await?;
    replace_people(
        pool,
        entry.id,
        input.cast.as_deref().unwrap_or_default(),
        "actor",
    )
    .await?;
    replace_people(
        pool,
        entry.id,
        input.directors.as_deref().unwrap_or_default(),
        "director",
    )
    .await?;

    Ok(entry)
}

pub async fn find_entry_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> anyhow::Result<Option<MovieEntry>> {
    let entry = sqlx::query_as::<_, MovieEntry>(&format!(
        "SELECT {ENTRY_COLUMNS} FROM movie_entry WHERE slug = ?"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(entry)
}

/// `draft=true` → 미발행 포함. 기본은 발행본만.
/// series_group_slug로 필터 가능.
/// 정렬: 최신 watched_at 우선, 없으면 최신 created_at. NULL은 뒤로.
pub async fn list_entries(
    pool: &SqlitePool,
    series_group_slug: Option<&str>,
    limit: i64,
    draft: bool,
) -> anyhow::Result<Vec<MovieEntry>> {
    let limit = limit.clamp(1, 1000);
    let published_clause = if draft {
        ""
    } else {
        "published_at IS NOT NULL"
    };
    let entries = if let Some(slug) = series_group_slug {
        let sql = if draft {
            format!(
                "SELECT {ENTRY_COLUMNS} FROM movie_entry
                 WHERE series_group_id = (SELECT id FROM series_group WHERE slug = ?)
                 ORDER BY COALESCE(watched_at, created_at) DESC, id DESC
                 LIMIT ?"
            )
        } else {
            format!(
                "SELECT {ENTRY_COLUMNS} FROM movie_entry
                 WHERE {published_clause}
                   AND series_group_id = (SELECT id FROM series_group WHERE slug = ?)
                 ORDER BY COALESCE(watched_at, created_at) DESC, id DESC
                 LIMIT ?"
            )
        };
        sqlx::query_as::<_, MovieEntry>(&sql)
            .bind(slug)
            .bind(limit)
            .fetch_all(pool)
            .await?
    } else {
        let sql = if draft {
            format!(
                "SELECT {ENTRY_COLUMNS} FROM movie_entry
                 ORDER BY COALESCE(watched_at, created_at) DESC, id DESC
                 LIMIT ?"
            )
        } else {
            format!(
                "SELECT {ENTRY_COLUMNS} FROM movie_entry
                 WHERE {published_clause}
                 ORDER BY COALESCE(watched_at, created_at) DESC, id DESC
                 LIMIT ?"
            )
        };
        sqlx::query_as::<_, MovieEntry>(&sql)
            .bind(limit)
            .fetch_all(pool)
            .await?
    };
    Ok(entries)
}

/// 하위 호환: 발행본만. 새 코드는 `list_entries(_, _, _, false)`를 직접 호출.
pub async fn list_entries_published(
    pool: &SqlitePool,
    series_group_slug: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<MovieEntry>> {
    list_entries(pool, series_group_slug, limit, false).await
}

/// 엔트리 + 장르 + 출연진/감독을 한 번에 조립 (목록/빌드용).
pub async fn list_entries_detail(
    pool: &SqlitePool,
    limit: i64,
    draft: bool,
) -> anyhow::Result<Vec<MovieEntryDetail>> {
    let entries = list_entries(pool, None, limit, draft).await?;
    assemble_details(pool, entries).await
}

/// 단일 엔트리 상세 조립 (show용).
pub async fn find_entry_detail_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> anyhow::Result<Option<MovieEntryDetail>> {
    let Some(entry) = find_entry_by_slug(pool, slug).await? else {
        return Ok(None);
    };
    Ok(Some(assemble_details(pool, vec![entry]).await?.remove(0)))
}

/// 엔트리 목록에 장르·인물을 일괄 조인해 상세로 만든다.
/// ≤200 엔트리 가정: movie_genre/movie_entry_person 전체를 한 번씩 읽고 HashMap 그룹화.
async fn assemble_details(
    pool: &SqlitePool,
    entries: Vec<MovieEntry>,
) -> anyhow::Result<Vec<MovieEntryDetail>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    #[derive(sqlx::FromRow)]
    struct GenreRow {
        movie_entry_id: i64,
        name_en: String,
        name_ko: Option<String>,
    }
    let genre_rows: Vec<GenreRow> =
        sqlx::query_as("SELECT movie_entry_id, name_en, name_ko FROM movie_genre")
            .fetch_all(pool)
            .await?;
    let mut genres: HashMap<i64, Vec<GenreName>> = HashMap::new();
    for r in genre_rows {
        genres.entry(r.movie_entry_id).or_default().push(GenreName {
            name_en: r.name_en,
            name_ko: r.name_ko,
        });
    }

    #[derive(sqlx::FromRow)]
    struct PersonRow {
        movie_entry_id: i64,
        id: i64,
        slug: String,
        name_en: String,
        name_ko: Option<String>,
        profile_path: Option<String>,
        role: String,
        character_name: Option<String>,
        billing: Option<i32>,
    }
    let person_rows: Vec<PersonRow> = sqlx::query_as(
        "SELECT mep.movie_entry_id, mp.id, mp.slug, mp.name_en, mp.name_ko,
                mp.profile_path, mp.role, mep.character_name, mep.billing
         FROM movie_entry_person mep
         JOIN movie_person mp ON mp.id = mep.person_id",
    )
    .fetch_all(pool)
    .await?;
    let mut cast: HashMap<i64, Vec<PersonSummary>> = HashMap::new();
    let mut directors: HashMap<i64, Vec<PersonSummary>> = HashMap::new();
    for r in person_rows {
        let summary = PersonSummary {
            id: r.id,
            slug: r.slug,
            name_en: r.name_en,
            name_ko: r.name_ko,
            profile_path: r.profile_path,
            role: r.role.clone(),
            character_name: r.character_name,
            billing: r.billing,
        };
        if r.role == "director" {
            directors.entry(r.movie_entry_id).or_default().push(summary);
        } else {
            cast.entry(r.movie_entry_id).or_default().push(summary);
        }
    }

    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let id = e.id;
        let mut entry_cast = cast.remove(&id).unwrap_or_default();
        entry_cast.sort_by_key(|p| p.billing.unwrap_or(i32::MAX));
        let entry_directors = directors.remove(&id).unwrap_or_default();
        out.push(MovieEntryDetail {
            entry: e,
            genres: genres.remove(&id).unwrap_or_default(),
            cast: entry_cast,
            directors: entry_directors,
        });
    }
    Ok(out)
}

/// 시리즈에 속한 entry (group_id 기반).
/// 정렬은 series_order ASC, NULLs last, id ASC.
/// 공개 API에서는 published_only=true로 호출 (초안 숨김).
pub async fn list_entries_by_group_id(
    pool: &SqlitePool,
    group_id: i64,
    published_only: bool,
) -> anyhow::Result<Vec<MovieEntry>> {
    let entries = if published_only {
        sqlx::query_as::<_, MovieEntry>(&format!(
            "SELECT {ENTRY_COLUMNS} FROM movie_entry
             WHERE series_group_id = ?
               AND published_at IS NOT NULL
             ORDER BY CASE WHEN series_order IS NULL THEN 1 ELSE 0 END,
                      series_order ASC, id ASC"
        ))
        .bind(group_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, MovieEntry>(&format!(
            "SELECT {ENTRY_COLUMNS} FROM movie_entry
             WHERE series_group_id = ?
             ORDER BY CASE WHEN series_order IS NULL THEN 1 ELSE 0 END,
                      series_order ASC, id ASC"
        ))
        .bind(group_id)
        .fetch_all(pool)
        .await?
    };
    Ok(entries)
}

pub async fn publish_entry(pool: &SqlitePool, slug: &str) -> anyhow::Result<MovieEntry> {
    let entry = sqlx::query_as::<_, MovieEntry>(&format!(
        "UPDATE movie_entry
            SET published_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
          WHERE slug = ?
         RETURNING {ENTRY_COLUMNS}"
    ))
    .bind(slug)
    .fetch_one(pool)
    .await?;
    Ok(entry)
}

/// 부분 갱신. None은 미변경.
pub async fn update_entry(
    pool: &SqlitePool,
    slug: &str,
    patch: &MovieEntryPatch,
) -> anyhow::Result<Option<MovieEntry>> {
    let mut sets: Vec<&str> = Vec::new();
    if patch.tmdb_id.is_some() {
        sets.push("tmdb_id = ?");
    }
    if patch.media_type.is_some() {
        sets.push("media_type = ?");
    }
    if patch.title.is_some() {
        sets.push("title = ?");
    }
    if patch.title_ko.is_some() {
        sets.push("title_ko = ?");
    }
    if patch.title_en.is_some() {
        sets.push("title_en = ?");
    }
    if patch.poster_path.is_some() {
        sets.push("poster_path = ?");
    }
    if patch.origin.is_some() {
        sets.push("origin = ?");
    }
    if patch.release_year.is_some() {
        sets.push("release_year = ?");
    }
    if patch.runtime_min.is_some() {
        sets.push("runtime_min = ?");
    }
    if patch.watched_at.is_some() {
        sets.push("watched_at = ?");
    }
    if patch.rating.is_some() {
        sets.push("rating = ?");
    }
    if patch.review_ko.is_some() {
        sets.push("review_ko = ?");
    }
    if patch.review_en.is_some() {
        sets.push("review_en = ?");
    }
    if patch.rewatch.is_some() {
        sets.push("rewatch = ?");
    }
    if patch.series_group_id.is_some() {
        sets.push("series_group_id = ?");
    }
    if patch.series_order.is_some() {
        sets.push("series_order = ?");
    }

    let has_relation_change =
        patch.genres.is_some() || patch.cast.is_some() || patch.directors.is_some();
    if sets.is_empty() && !has_relation_change {
        return find_entry_by_slug(pool, slug).await;
    }

    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')");
    let set_clause = sets.join(", ");

    let sql =
        format!("UPDATE movie_entry SET {set_clause} WHERE slug = ? RETURNING {ENTRY_COLUMNS}");
    let mut q = sqlx::query_as::<_, MovieEntry>(&sql);
    if let Some(v) = &patch.tmdb_id {
        q = q.bind(*v);
    }
    if let Some(v) = &patch.media_type {
        q = q.bind(v);
    }
    if let Some(v) = &patch.title {
        q = q.bind(v);
    }
    if let Some(v) = &patch.title_ko {
        q = q.bind(v);
    }
    if let Some(v) = &patch.title_en {
        q = q.bind(v);
    }
    if let Some(v) = &patch.poster_path {
        q = q.bind(v);
    }
    if let Some(v) = &patch.origin {
        q = q.bind(v);
    }
    if let Some(v) = patch.release_year {
        q = q.bind(v);
    }
    if let Some(v) = patch.runtime_min {
        q = q.bind(v);
    }
    if let Some(v) = &patch.watched_at {
        q = q.bind(v);
    }
    if let Some(v) = patch.rating {
        q = q.bind(v);
    }
    if let Some(v) = &patch.review_ko {
        q = q.bind(v);
    }
    if let Some(v) = &patch.review_en {
        q = q.bind(v);
    }
    if let Some(v) = patch.rewatch {
        q = q.bind(if v { 1i8 } else { 0i8 });
    }
    if let Some(v) = patch.series_group_id {
        q = q.bind(v);
    }
    if let Some(v) = patch.series_order {
        q = q.bind(v);
    }
    let entry = q.bind(slug).fetch_optional(pool).await?;

    // 관계(장르/출연진/감독)는 행 갱신 후 별도 동기화.
    if let Some(entry) = &entry {
        if let Some(genres) = &patch.genres {
            replace_genres(pool, entry.id, genres).await?;
        }
        if let Some(cast) = &patch.cast {
            replace_people(pool, entry.id, cast, "actor").await?;
        }
        if let Some(directors) = &patch.directors {
            replace_people(pool, entry.id, directors, "director").await?;
        }
    }

    Ok(entry)
}

pub async fn delete_entry(pool: &SqlitePool, slug: &str) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM movie_entry WHERE slug = ?")
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

// ─── Genres & people sync ───

/// 엔트리의 장르를 전체 교체. 빈 name_en 은 name_ko 로, 둘 다 비면 건너뛴다.
pub async fn replace_genres(
    pool: &SqlitePool,
    entry_id: i64,
    genres: &[GenreInput],
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM movie_genre WHERE movie_entry_id = ?")
        .bind(entry_id)
        .execute(pool)
        .await?;
    for g in genres {
        let en = g
            .name_en
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                g.name_ko
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            });
        let Some(en) = en else { continue };
        let ko = g
            .name_ko
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != en);
        sqlx::query(
            "INSERT OR IGNORE INTO movie_genre (movie_entry_id, name_en, name_ko) VALUES (?, ?, ?)",
        )
        .bind(entry_id)
        .bind(en)
        .bind(ko)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 엔트리의 특정 role 인물을 전체 교체.
/// role 이 같은 기존 join 행만 지운다 (cast/director 독립 갱신).
pub async fn replace_people(
    pool: &SqlitePool,
    entry_id: i64,
    people: &[PersonInput],
    role: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "DELETE FROM movie_entry_person
         WHERE movie_entry_id = ?
           AND person_id IN (SELECT id FROM movie_person WHERE role = ?)",
    )
    .bind(entry_id)
    .bind(role)
    .execute(pool)
    .await?;
    for p in people {
        let resolved_role = p.role.as_deref().unwrap_or(role);
        let person_id = upsert_person(pool, p, resolved_role).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO movie_entry_person
             (movie_entry_id, person_id, character_name, billing) VALUES (?, ?, ?, ?)",
        )
        .bind(entry_id)
        .bind(person_id)
        .bind(
            p.character_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        )
        .bind(p.billing)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 인물 upsert. tmdb_person_id → name_en+role → 신규 생성 순.
async fn upsert_person(pool: &SqlitePool, p: &PersonInput, role: &str) -> anyhow::Result<i64> {
    let name_en = p
        .name_en
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            p.name_ko
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .ok_or_else(|| anyhow!("person name_en or name_ko required"))?
        .to_string();
    let name_ko = p
        .name_ko
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != name_en);

    // 1) tmdb id 매칭 시 갱신.
    if let Some(tid) = p.tmdb_person_id {
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM movie_person WHERE tmdb_person_id = ?")
                .bind(tid)
                .fetch_optional(pool)
                .await?;
        if let Some((id,)) = existing {
            sqlx::query(
                "UPDATE movie_person
                    SET name_ko = COALESCE(?, name_ko),
                        profile_path = COALESCE(?, profile_path)
                  WHERE id = ?",
            )
            .bind(name_ko)
            .bind(p.profile_path.as_deref())
            .bind(id)
            .execute(pool)
            .await?;
            return Ok(id);
        }
    }

    // 2) 수동 중복 (같은 name_en + role, tmdb 없음).
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM movie_person
         WHERE name_en = ? AND role = ? AND tmdb_person_id IS NULL",
    )
    .bind(&name_en)
    .bind(role)
    .fetch_optional(pool)
    .await?;
    if let Some((id,)) = existing {
        return Ok(id);
    }

    // 3) 신규 생성.
    let base_slug = p
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| person_slugify(&name_en));
    let slug = ensure_unique_person_slug(pool, &base_slug).await?;
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO movie_person
            (tmdb_person_id, slug, name_en, name_ko, profile_path, role)
         VALUES (?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(p.tmdb_person_id)
    .bind(&slug)
    .bind(&name_en)
    .bind(name_ko)
    .bind(p.profile_path.as_deref())
    .bind(role)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// ─── SeriesGroup ───

pub async fn create_group(
    pool: &SqlitePool,
    input: &SeriesGroupInput,
    resolved_slug: &str,
) -> anyhow::Result<SeriesGroup> {
    let group = sqlx::query_as::<_, SeriesGroup>(&format!(
        "INSERT INTO series_group
            (slug, title_ko, title_en, cover_image,
             group_rating, group_review_ko, group_review_en)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         RETURNING {GROUP_COLUMNS}"
    ))
    .bind(resolved_slug)
    .bind(input.title_ko.as_deref())
    .bind(input.title_en.as_deref())
    .bind(input.cover_image.as_deref())
    .bind(input.group_rating)
    .bind(input.group_review_ko.as_deref())
    .bind(input.group_review_en.as_deref())
    .fetch_one(pool)
    .await?;
    Ok(group)
}

pub async fn find_group_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> anyhow::Result<Option<SeriesGroup>> {
    let group = sqlx::query_as::<_, SeriesGroup>(&format!(
        "SELECT {GROUP_COLUMNS} FROM series_group WHERE slug = ?"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(group)
}

pub async fn update_group(
    pool: &SqlitePool,
    slug: &str,
    patch: &SeriesGroupPatch,
) -> anyhow::Result<Option<SeriesGroup>> {
    let mut sets: Vec<&str> = Vec::new();
    if patch.title_ko.is_some() {
        sets.push("title_ko = ?");
    }
    if patch.title_en.is_some() {
        sets.push("title_en = ?");
    }
    if patch.cover_image.is_some() {
        sets.push("cover_image = ?");
    }
    if patch.group_rating.is_some() {
        sets.push("group_rating = ?");
    }
    if patch.group_review_ko.is_some() {
        sets.push("group_review_ko = ?");
    }
    if patch.group_review_en.is_some() {
        sets.push("group_review_en = ?");
    }
    if sets.is_empty() {
        return find_group_by_slug(pool, slug).await;
    }
    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')");
    let set_clause = sets.join(", ");

    let sql =
        format!("UPDATE series_group SET {set_clause} WHERE slug = ? RETURNING {GROUP_COLUMNS}");
    let mut q = sqlx::query_as::<_, SeriesGroup>(&sql);
    if let Some(v) = &patch.title_ko {
        q = q.bind(v);
    }
    if let Some(v) = &patch.title_en {
        q = q.bind(v);
    }
    if let Some(v) = &patch.cover_image {
        q = q.bind(v);
    }
    if let Some(v) = patch.group_rating {
        q = q.bind(v);
    }
    if let Some(v) = &patch.group_review_ko {
        q = q.bind(v);
    }
    if let Some(v) = &patch.group_review_en {
        q = q.bind(v);
    }
    let group = q.bind(slug).fetch_optional(pool).await?;
    Ok(group)
}

pub async fn delete_group(pool: &SqlitePool, slug: &str) -> anyhow::Result<bool> {
    let group = find_group_by_slug(pool, slug).await?;
    let id = match group {
        Some(g) => g.id,
        None => return Ok(false),
    };
    sqlx::query("UPDATE movie_entry SET series_group_id = NULL WHERE series_group_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    let res = sqlx::query("DELETE FROM series_group WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn list_groups(pool: &SqlitePool) -> anyhow::Result<Vec<SeriesGroup>> {
    let groups = sqlx::query_as::<_, SeriesGroup>(&format!(
        "SELECT {GROUP_COLUMNS} FROM series_group ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(groups)
}
