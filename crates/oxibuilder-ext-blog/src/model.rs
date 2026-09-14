use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BlogPost {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub body: String,
    pub lang: String,
    pub translation_group_id: Option<i64>,
    #[sqlx(json)]
    pub tags: Vec<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub category: Option<String>,
    pub series_id: Option<i64>,
    pub series_order: Option<i64>,
}

/// POST 입력. published_at은 받지 않는다 (초안 우선 원칙).
/// `series`는 slug로 받는다 — repo::create가 id로 해소한다.
#[derive(Debug, Clone, Deserialize)]
pub struct BlogPostInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub translation_group_id: Option<i64>,
    /// 사용자 명시 slug. 미지정 시 title로부터 자동 생성.
    pub slug: Option<String>,
    pub category: Option<String>,
    pub series: Option<String>,
    pub series_order: Option<i64>,
}

fn default_lang() -> String {
    "ko".into()
}

/// PATCH 입력. 전부 Option.
/// 이중 Option: 바깥 None = 미수정, Some(None) = NULL로 초기화,
/// Some(Some(v)) = v로 변경.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BlogPatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub lang: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<Option<String>>,
    pub series: Option<Option<String>>,
    pub series_order: Option<Option<i64>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub draft: bool,
    pub lang: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BlogSeries {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

/// POST 입력. slug 미지정 시 title로부터 자동 생성.
#[derive(Debug, Clone, Deserialize)]
pub struct BlogSeriesInput {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub slug: Option<String>,
}

/// GET /series/{slug} 응답 — 시리즈 메타 + 발행 순서로 정렬된 글.
#[derive(Debug, Clone, Serialize)]
pub struct BlogSeriesDetail {
    #[serde(flatten)]
    pub series: BlogSeries,
    pub posts: Vec<BlogPost>,
}
