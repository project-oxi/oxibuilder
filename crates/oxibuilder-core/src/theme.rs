//! Single source of truth for the curated public site theme catalog.
//!
//! Both the setup wizard, the public catalog endpoint, the per-site theme
//! PUT validator, the Admin ThemesPage, and the browser-side
//! `applyServerTheme` consume this catalog. There is no other catalog.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ThemeDefinition {
    pub id: &'static str,
    pub name_ko: &'static str,
    pub name_en: &'static str,
    pub mode: ThemeMode,
    pub accent_hue: f64,
    pub preview_colors: [&'static str; 4],
    pub description_ko: &'static str,
    pub description_en: &'static str,
}

/// The shared catalog. Order is the order shown in Admin → Themes and the
/// setup wizard. Mode/hue/preview/description are complete for every entry.
pub const ALL_THEMES: &[ThemeDefinition] = &[
    ThemeDefinition {
        id: "paper",
        name_ko: "종이",
        name_en: "Paper",
        mode: ThemeMode::Light,
        accent_hue: 160.0,
        preview_colors: ["#fafaf5", "#f5f2ed", "#2d2934", "#2d7a5c"],
        description_ko: "따뜻한 종이 배경, 파인 그린 악센트",
        description_en: "Warm paper background, pine green accent",
    },
    ThemeDefinition {
        id: "midnight",
        name_ko: "한밤",
        name_en: "Midnight",
        mode: ThemeMode::Dark,
        accent_hue: 250.0,
        preview_colors: ["#1a1a2e", "#16213e", "#e0e0e0", "#4fc3f7"],
        description_ko: "깊은 밤하늘, 블루 악센트",
        description_en: "Deep night sky, blue accent",
    },
    ThemeDefinition {
        id: "sepia",
        name_ko: "세피아",
        name_en: "Sepia",
        mode: ThemeMode::Light,
        accent_hue: 70.0,
        preview_colors: ["#f5f0e8", "#ede0d4", "#3d3529", "#b8860b"],
        description_ko: "오래된 책장, 앰버-골드 악센트",
        description_en: "Old bookshelf, amber-gold accent",
    },
    ThemeDefinition {
        id: "forest",
        name_ko: "숲",
        name_en: "Forest",
        mode: ThemeMode::Dark,
        accent_hue: 155.0,
        preview_colors: ["#1b2b1b", "#243624", "#e0e8e0", "#2ecc71"],
        description_ko: "이끼 낀 숲, 에메랄드 악센트",
        description_en: "Mossy forest, emerald accent",
    },
    ThemeDefinition {
        id: "neon",
        name_ko: "네온",
        name_en: "Neon",
        mode: ThemeMode::Dark,
        accent_hue: 290.0,
        preview_colors: ["#0d0221", "#16003b", "#f4e6ff", "#a855f7"],
        description_ko: "합성 보라, 마젠타-네온 악센트",
        description_en: "Synthetic purple, magenta-neon accent",
    },
    ThemeDefinition {
        id: "canvas",
        name_ko: "캔버스",
        name_en: "Canvas",
        mode: ThemeMode::Light,
        accent_hue: 240.0,
        preview_colors: ["#fdfdfb", "#f4f4f1", "#1f2937", "#0ea5e9"],
        description_ko: "화이트 캔버스, 스카이-블루 악센트",
        description_en: "White canvas, sky-blue accent",
    },
];

pub fn find_theme(id: &str) -> Option<&'static ThemeDefinition> {
    ALL_THEMES.iter().find(|t| t.id == id)
}

pub fn is_known_theme(id: &str) -> bool {
    find_theme(id).is_some()
}

/// Read the active theme id for a site from its per-site DB
/// (`theme_config.theme_id` singleton row id=1). Returns `"paper"` if the
/// table/row is absent or the read fails — never blocks a build.
pub async fn active_theme_id(db: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT theme_id FROM theme_config WHERE id = 1")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "paper".into())
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct LayoutDefinition {
    pub id: &'static str,
    pub name_ko: &'static str,
    pub name_en: &'static str,
    pub description_ko: &'static str,
    pub description_en: &'static str,
}

pub const ALL_LAYOUTS: &[LayoutDefinition] = &[
    LayoutDefinition {
        id: "shell",
        name_ko: "셸",
        name_en: "Shell",
        description_ko: "스티키 헤더·네비·푸터, 그리드/캔버스 로비",
        description_en: "Sticky header/nav/footer, grid/canvas lobby",
    },
    LayoutDefinition {
        id: "editorial",
        name_ko: "에디토리얼",
        name_en: "Editorial",
        description_ko: "크롬 없음, 중앙 허브 로비, 페이지별 헤더",
        description_en: "No chrome, centered hub lobby, per-page headers",
    },
];

pub fn find_layout(id: &str) -> Option<&'static LayoutDefinition> {
    ALL_LAYOUTS.iter().find(|l| l.id == id)
}

pub fn is_known_layout(id: &str) -> bool {
    find_layout(id).is_some()
}

/// Read the active layout for a site. Falls back to `default` (from config)
/// when the table/row/column is absent — never blocks a build.
pub async fn active_layout_id(db: &sqlx::SqlitePool, default: &str) -> String {
    let row: Option<(String,)> = sqlx::query_as("SELECT layout FROM theme_config WHERE id = 1")
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    match row {
        Some((l,)) if is_known_layout(&l) => l,
        _ => default.to_string(),
    }
}

#[test]
fn layout_catalog_is_complete() {
    assert_eq!(ALL_LAYOUTS.len(), 2);
    assert!(is_known_layout("shell"));
    assert!(is_known_layout("editorial"));
    assert!(!is_known_layout("bogus"));
    assert_eq!(find_layout("editorial").unwrap().id, "editorial");
}
