//! Lobby manifest assembly — single source of truth for the shape the public SPA fetches
//! via `fetchManifest()`.
//!
//! Used by two consumers that MUST produce the identical contract:
//! - the live HTTP handler `GET /api/console/lobby/manifest` (`crate::http`), and
//! - the SSG build, which writes `data/lobby.json` for static mode (`oxibuilder build`).
//!
//! Keeping one assembly function prevents the static site from drifting out of sync with the
//! live API (the exact bug that left the static lobby rendering no cards).

use crate::config::Config;
use crate::extension::{Extension, Lang};
use serde::Serialize;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ManifestSite {
    pub name: String,
    pub base_url: String,
    pub default_lang: String,
    pub languages: Vec<String>,
    pub layout: String,
}

#[derive(Serialize, Clone)]
pub struct ManifestLocalized {
    pub ko: String,
    pub en: String,
}

#[derive(Serialize, Clone, Default)]
pub struct LobbyConfigInfo {
    pub enabled: bool,
    pub display_mode: String,
    pub display_order: i64,
    pub style_params: serde_json::Value,
}

#[derive(Serialize)]
pub struct ManifestExtension {
    pub id: String,
    pub display_name: ManifestLocalized,
    pub lobby: LobbyConfigInfo,
}

#[derive(Serialize)]
pub struct ManifestMount {
    pub id: String,
    pub display_name: ManifestLocalized,
    pub path: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub open_in_new_tab: bool,
}

#[derive(Serialize)]
pub struct Manifest {
    pub site: ManifestSite,
    pub extensions: Vec<ManifestExtension>,
    pub mounts: Vec<ManifestMount>,
}

/// Per-extension lobby display config from the `lobby_config` table, falling back to the
/// config default mode + a synthesized order when no row exists yet.
pub async fn lobby_config_for(
    db: &SqlitePool,
    config: &Config,
    ext_id: &str,
    default_order: i64,
) -> LobbyConfigInfo {
    let row: Option<(bool, String, i64, String)> = sqlx::query_as(
        "SELECT enabled, display_mode, display_order, style_params
         FROM lobby_config WHERE extension_id = ?",
    )
    .bind(ext_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    match row {
        Some((enabled, mode, order, params)) => LobbyConfigInfo {
            enabled,
            display_mode: mode,
            display_order: order,
            style_params: serde_json::from_str(&params).unwrap_or_default(),
        },
        None => LobbyConfigInfo {
            enabled: true,
            display_mode: config.lobby.default_mode.clone(),
            display_order: default_order,
            style_params: serde_json::json!({}),
        },
    }
}

/// Assemble the full manifest: resolved site metadata + every active extension with its lobby
/// config.
///
/// `site_name` / `base_url` are resolved by the caller — the live handler honors a runtime
/// site override, while the build uses the config values directly (no override at build time).
/// Extensions disabled or purged in `extension_state` are omitted, mirroring the route gate.
pub async fn assemble(
    db: &SqlitePool,
    config: &Config,
    site_name: &str,
    base_url: &str,
    extensions: &[Arc<dyn Extension>],
) -> Manifest {
    let layout = crate::theme::active_layout_id(db, &config.lobby.layout).await;
    let mut ext_list = Vec::with_capacity(extensions.len());
    for (idx, e) in extensions.iter().enumerate() {
        if !is_active(db, e.id()).await {
            continue;
        }
        let lobby = lobby_config_for(db, config, e.id(), idx as i64).await;
        ext_list.push(ManifestExtension {
            id: e.id().to_string(),
            display_name: ManifestLocalized {
                ko: e.display_name(Lang::Ko),
                en: e.display_name(Lang::En),
            },
            lobby,
        });
    }
    Manifest {
        site: ManifestSite {
            name: site_name.to_string(),
            base_url: base_url.to_string(),
            default_lang: config.site.default_lang.clone(),
            languages: config.site.languages.clone(),
            layout,
        },
        extensions: ext_list,
        mounts: manifest_mounts(&config.mounts),
    }
}

/// Whether an extension's routes are live: `extension_state.enabled && !purged`.
///
/// A missing row is treated as active: the build may run on a DB that has not yet been seeded
/// (first boot seeds rows from `[extensions].enabled`), and silently dropping content there
/// would be worse than including it. Once seeded, the row authoritatively gates inclusion.
async fn is_active(db: &SqlitePool, ext_id: &str) -> bool {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT enabled, purged FROM extension_state WHERE extension_id = ?")
            .bind(ext_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    match row {
        Some((enabled, purged)) => enabled != 0 && purged == 0,
        None => true,
    }
}

/// Map configured static mounts to their manifest representation. Pure
/// (no DB) so it can be unit-tested in isolation; `assemble` calls it.
pub fn manifest_mounts(mounts: &[crate::config::MountConfig]) -> Vec<ManifestMount> {
    mounts
        .iter()
        .filter(|m| !m.hidden)
        .map(|m| ManifestMount {
            id: m.id.clone(),
            display_name: ManifestLocalized {
                ko: m.title_ko.clone(),
                en: m.title_en.clone(),
            },
            path: m.path.trim_matches('/').to_string(),
            description: m.description.clone(),
            icon: m.icon.clone(),
            open_in_new_tab: m.open_in_new_tab,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MountConfig;

    fn mc(id: &str, path: &str, ko: &str, en: &str) -> MountConfig {
        MountConfig {
            id: id.into(),
            source: format!("/srv/{id}").into(),
            path: path.into(),
            title_ko: ko.into(),
            title_en: en.into(),
            description: Some("desc".into()),
            icon: None,
            open_in_new_tab: true,
            hidden: false,
            raw: false,
        }
    }

    #[test]
    fn manifest_mounts_maps_config_fields() {
        let ms = manifest_mounts(&[mc("portfolio", "portfolio", "포트폴리오", "Portfolio")]);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].id, "portfolio");
        assert_eq!(ms[0].display_name.ko, "포트폴리오");
        assert_eq!(ms[0].display_name.en, "Portfolio");
        assert_eq!(ms[0].path, "portfolio");
        assert_eq!(ms[0].description.as_deref(), Some("desc"));
        assert!(ms[0].open_in_new_tab);
    }

    #[test]
    fn manifest_mounts_normalizes_path() {
        let ms = manifest_mounts(&[mc("p", "/stuff/", "k", "e")]);
        assert_eq!(ms[0].path, "stuff");
    }

    #[test]
    fn manifest_mounts_skips_hidden() {
        let mut hid = mc("posters", "posters", "포스터", "Posters");
        hid.hidden = true;
        let ms = manifest_mounts(&[mc("blog", "blog", "블로그", "Blog"), hid]);
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].id, "blog");
    }

    #[test]
    fn manifest_mounts_empty_for_no_config() {
        assert!(manifest_mounts(&[]).is_empty());
    }

    #[test]
    fn manifest_site_serializes_layout() {
        let site = ManifestSite {
            name: "Example".into(),
            base_url: "https://example.com/".into(),
            default_lang: "en".into(),
            languages: vec!["en".into()],
            layout: "editorial".into(),
        };

        let value = serde_json::to_value(site).unwrap();
        assert_eq!(value["layout"], "editorial");
    }
}
