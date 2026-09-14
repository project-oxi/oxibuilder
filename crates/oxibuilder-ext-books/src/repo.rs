use crate::model::{Book, BookInput, BookPatch};
use sqlx::SqlitePool;

const COLUMNS: &str = "id, source, external_id, isbn13, title, author, cover_image_url,
                       category, publisher, page_count,
                       rating, review_ko, review_en, status, started_at, finished_at,
                       published_at, created_at, updated_at";

pub async fn create(pool: &SqlitePool, input: &BookInput) -> anyhow::Result<Book> {
    let row = sqlx::query_as::<_, Book>(&format!(
        "INSERT INTO book_entry
            (source, external_id, isbn13, title, author, cover_image_url,
             category, publisher, page_count,
             rating, review_ko, review_en, status, started_at, finished_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         RETURNING {COLUMNS}"
    ))
    .bind(&input.source)
    .bind(&input.external_id)
    .bind(&input.isbn13)
    .bind(&input.title)
    .bind(&input.author)
    .bind(&input.cover_image_url)
    .bind(&input.category)
    .bind(&input.publisher)
    .bind(input.page_count)
    .bind(input.rating)
    .bind(&input.review_ko)
    .bind(&input.review_en)
    .bind(&input.status)
    .bind(&input.started_at)
    .bind(&input.finished_at)
    .fetch_one(pool)
    .await?;
    Ok(row.normalize_status())
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Book>> {
    let row = sqlx::query_as::<_, Book>(&format!("SELECT {COLUMNS} FROM book_entry WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(Book::normalize_status))
}

/// `status=None` → 전체. `draft=true` → 미발행 행 포함. 기본은 발행본만.
pub async fn list(
    pool: &SqlitePool,
    status: Option<&str>,
    limit: i64,
    draft: bool,
) -> anyhow::Result<Vec<Book>> {
    let limit = limit.clamp(1, 1000);
    let published_clause = if draft {
        ""
    } else {
        "published_at IS NOT NULL"
    };
    let sql = if status.is_some() {
        if draft {
            format!(
                "SELECT {COLUMNS} FROM book_entry
                 WHERE status = ?
                 ORDER BY COALESCE(published_at, created_at) DESC LIMIT ?"
            )
        } else {
            format!(
                "SELECT {COLUMNS} FROM book_entry
                 WHERE {published_clause} AND status = ?
                 ORDER BY published_at DESC LIMIT ?"
            )
        }
    } else if draft {
        format!(
            "SELECT {COLUMNS} FROM book_entry
             ORDER BY COALESCE(published_at, created_at) DESC LIMIT ?"
        )
    } else {
        format!(
            "SELECT {COLUMNS} FROM book_entry
             WHERE {published_clause}
             ORDER BY published_at DESC LIMIT ?"
        )
    };
    let mut q = sqlx::query_as::<_, Book>(&sql);
    if let Some(s) = status {
        q = q.bind(s);
    }
    let rows = q.bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(Book::normalize_status).collect())
}

pub async fn update(pool: &SqlitePool, id: i64, patch: &BookPatch) -> anyhow::Result<Option<Book>> {
    let mut sets: Vec<&str> = Vec::new();
    if patch.source.is_some() {
        sets.push("source = ?");
    }
    if patch.external_id.is_some() {
        sets.push("external_id = ?");
    }
    if patch.isbn13.is_some() {
        sets.push("isbn13 = ?");
    }
    if patch.title.is_some() {
        sets.push("title = ?");
    }
    if patch.author.is_some() {
        sets.push("author = ?");
    }
    if patch.cover_image_url.is_some() {
        sets.push("cover_image_url = ?");
    }
    if patch.category.is_some() {
        sets.push("category = ?");
    }
    if patch.publisher.is_some() {
        sets.push("publisher = ?");
    }
    if patch.page_count.is_some() {
        sets.push("page_count = ?");
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
    if patch.status.is_some() {
        sets.push("status = ?");
    }
    if patch.started_at.is_some() {
        sets.push("started_at = ?");
    }
    if patch.finished_at.is_some() {
        sets.push("finished_at = ?");
    }
    if sets.is_empty() {
        return find_by_id(pool, id).await;
    }
    sets.push("updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')");
    let set_clause = sets.join(", ");
    let sql = format!("UPDATE book_entry SET {set_clause} WHERE id = ? RETURNING {COLUMNS}");
    let mut q = sqlx::query_as::<_, Book>(&sql);
    if let Some(v) = &patch.source {
        q = q.bind(v);
    }
    if let Some(v) = &patch.external_id {
        q = q.bind(v);
    }
    if let Some(v) = &patch.isbn13 {
        q = q.bind(v);
    }
    if let Some(v) = &patch.title {
        q = q.bind(v);
    }
    if let Some(v) = &patch.author {
        q = q.bind(v);
    }
    if let Some(v) = &patch.cover_image_url {
        q = q.bind(v);
    }
    if let Some(v) = &patch.category {
        q = q.bind(v);
    }
    if let Some(v) = &patch.publisher {
        q = q.bind(v);
    }
    if let Some(v) = patch.page_count {
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
    if let Some(v) = &patch.status {
        q = q.bind(v);
    }
    if let Some(v) = &patch.started_at {
        q = q.bind(v);
    }
    if let Some(v) = &patch.finished_at {
        q = q.bind(v);
    }
    let row = q.bind(id).fetch_optional(pool).await?;
    Ok(row.map(Book::normalize_status))
}

pub async fn publish(pool: &SqlitePool, id: i64) -> anyhow::Result<Book> {
    let row = sqlx::query_as::<_, Book>(&format!(
        "UPDATE book_entry
            SET published_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
          WHERE id = ?
         RETURNING {COLUMNS}"
    ))
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(row.normalize_status())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM book_entry WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxibuilder_core::extension::Extension;

    /// CHECK 제약 우회가 필요한 레거시 행 삽입용 풀. 프로덕션 마이그레이션은
    /// 4값만 허용하므로 PRAGMA로 우회한다 (max_connections=1이라 단일 커넥션 적용).
    async fn test_pool() -> SqlitePool {
        let pool = oxibuilder_core::db::connect_memory().await.unwrap();
        sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&pool)
            .await
            .unwrap();
        for m in crate::BooksExtension.migrations() {
            sqlx::query(m.sql).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn list_normalizes_legacy_status() {
        let pool = test_pool().await;
        sqlx::query("INSERT INTO book_entry (title, status) VALUES ('Legacy Read', 'read')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO book_entry (title, status) VALUES ('Legacy Dnf', 'dnf')")
            .execute(&pool)
            .await
            .unwrap();

        let books = list(&pool, None, 10, true).await.unwrap();
        let read_book = books.iter().find(|b| b.title == "Legacy Read").unwrap();
        let dnf_book = books.iter().find(|b| b.title == "Legacy Dnf").unwrap();
        assert_eq!(read_book.status, "completed");
        assert_eq!(dnf_book.status, "dropped");
    }
}
