//! Runtime-mutable site settings (display, languages, lobby, integrations,
//! extensions, deploy). Server host/port/data_dir are intentionally excluded
//! — they are startup-immutable and captured once in `SiteContext::startup_server`.

use serde::{Deserialize, Serialize};

use crate::config::{
    Config, ExtensionsConfig, IntegrationsConfig, LobbySection, MountConfig, ServerConfig,
    SiteConfig,
};

/// Live-reloadable subset of site configuration. Excludes `[server]` fields
/// (host/port/data_dir) which are captured once at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutableSiteSettings {
    pub site: MutableSiteConfig,
    pub lobby: MutableLobbyConfig,
    pub integrations: MutableIntegrationsConfig,
    pub extensions: MutableExtensionsConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    /// Configured static mounts. Runtime data, not exposed through the
    /// settings API (#[serde(skip)]) — mounts are config-driven, not
    /// edited through the settings UI. Preserved across `from_config` /
    /// `to_config` round trips so `per_site.rs`'s reconstructed
    /// `AppState` keeps its mounts.
    #[serde(skip)]
    pub mounts: Vec<MountConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutableSiteConfig {
    pub name: String,
    pub base_url: String,
    pub default_lang: String,
    pub languages: Vec<String>,
    #[serde(default)]
    pub tagline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutableLobbyConfig {
    pub default_mode: String,
    pub layout: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MutableIntegrationsConfig {
    #[serde(default)]
    pub github_username: Option<String>,
    #[serde(default)]
    pub tmdb_api_key_env: Option<String>,
    #[serde(default)]
    pub aladin_ttbkey_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MutableExtensionsConfig {
    #[serde(default)]
    pub enabled: Vec<String>,
}

/// Live-reloadable deploy target. The console UI exposes this; the deploy
/// subproject reads it from the in-memory snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DeployConfig {
    #[serde(default)]
    pub github_pages: Option<GitHubPagesTarget>,
}

/// GitHub Pages target. `owner` and `repo` accept a permissive allowlist
/// (ASCII alphanumeric + `-`, `_`, `.`). `branch` defaults to `gh-pages`
/// and supports nested refs (e.g. `pages/v1`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubPagesTarget {
    pub owner: String,
    pub repo: String,
    #[serde(default = "default_pages_branch")]
    pub branch: String,
}

fn default_pages_branch() -> String {
    "gh-pages".into()
}

/// Per-field validation result for [`GitHubPagesTarget`]. Surfaced
/// verbatim to the user so they can fix the offending component.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TargetValidationError {
    #[error("invalid GitHub owner")]
    Owner,
    #[error("invalid GitHub repository")]
    Repo,
    #[error("invalid Git branch")]
    Branch,
}

impl GitHubPagesTarget {
    /// Validate each component against the shared allowlist. `owner`/`repo`
    /// accept non-empty ASCII alphanumeric, `-`, `_`, `.`. `branch` accepts
    /// the same plus `/`, rejects `..`, and rejects leading/trailing `/` so
    /// the path can never escape the branch namespace.
    pub fn validate(&self) -> Result<(), TargetValidationError> {
        let component = |v: &str| {
            !v.is_empty()
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        if !component(&self.owner) {
            return Err(TargetValidationError::Owner);
        }
        if !component(&self.repo) {
            return Err(TargetValidationError::Repo);
        }
        let branch_ok = !self.branch.is_empty()
            && !self.branch.contains("..")
            && !self.branch.starts_with('/')
            && !self.branch.ends_with('/')
            && self
                .branch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'));
        if !branch_ok {
            return Err(TargetValidationError::Branch);
        }
        Ok(())
    }

    /// Public Pages URL for this target. Root repos (`<owner>.github.io`) are
    /// served at the apex; project repos at `/<repo>/`.
    pub fn pages_url(&self) -> String {
        if self
            .repo
            .eq_ignore_ascii_case(&format!("{}.github.io", self.owner))
        {
            format!("https://{}.github.io/", self.owner)
        } else {
            format!("https://{}.github.io/{}/", self.owner, self.repo)
        }
    }

    /// Serve base path for this target. Root repos → `/`; project repos → `/<repo>/`.
    pub fn base_path(&self) -> String {
        if self
            .repo
            .eq_ignore_ascii_case(&format!("{}.github.io", self.owner))
        {
            "/".into()
        } else {
            format!("/{}/", self.repo)
        }
    }
}

impl MutableSiteSettings {
    /// Extract mutable settings from a full [`Config`].
    pub fn from_config(cfg: &Config) -> Self {
        MutableSiteSettings {
            site: MutableSiteConfig {
                name: cfg.site.name.clone(),
                base_url: cfg.site.base_url.clone(),
                default_lang: cfg.site.default_lang.clone(),
                languages: cfg.site.languages.clone(),
                tagline: cfg.site.tagline.clone(),
            },
            lobby: MutableLobbyConfig {
                default_mode: cfg.lobby.default_mode.clone(),
                layout: cfg.lobby.layout.clone(),
            },
            integrations: MutableIntegrationsConfig {
                github_username: cfg.integrations.github_username.clone(),
                tmdb_api_key_env: cfg.integrations.tmdb_api_key_env.clone(),
                aladin_ttbkey_env: cfg.integrations.aladin_ttbkey_env.clone(),
            },
            extensions: MutableExtensionsConfig {
                enabled: cfg.extensions.enabled.clone(),
            },
            deploy: cfg.deploy.clone(),
            mounts: cfg.mounts.clone(),
        }
    }

    /// Reconstruct a full [`Config`] from this snapshot plus the
    /// startup-immutable server section. Used for the legacy
    /// [`crate::state::AppState`] construction on the cold extension-enable
    /// path, where extensions still expect an `Arc<Config>`.
    pub fn to_config(&self, server: &ServerConfig) -> Config {
        Config {
            site: SiteConfig {
                name: self.site.name.clone(),
                base_url: self.site.base_url.clone(),
                default_lang: self.site.default_lang.clone(),
                languages: self.site.languages.clone(),
                tagline: self.site.tagline.clone(),
            },
            server: server.clone(),
            extensions: ExtensionsConfig {
                enabled: self.extensions.enabled.clone(),
            },
            integrations: IntegrationsConfig {
                github_username: self.integrations.github_username.clone(),
                tmdb_api_key_env: self.integrations.tmdb_api_key_env.clone(),
                aladin_ttbkey_env: self.integrations.aladin_ttbkey_env.clone(),
            },
            lobby: LobbySection {
                default_mode: self.lobby.default_mode.clone(),
                layout: self.lobby.layout.clone(),
            },
            deploy: self.deploy.clone(),
            mounts: self.mounts.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn from_config_then_to_config_round_trips_mutable_fields() {
        let mut cfg = Config::default();
        cfg.site.name = "Round".into();
        cfg.site.base_url = "https://example.test".into();
        cfg.site.default_lang = "en".into();
        cfg.site.languages = vec!["en".into(), "ko".into()];
        cfg.lobby.default_mode = "list".into();
        cfg.lobby.layout = "editorial".into();
        cfg.extensions.enabled = vec!["blog".into()];
        cfg.integrations.github_username = Some("octocat".into());
        cfg.integrations.tmdb_api_key_env = Some("OXIBUILDER_TMDB_KEY".into());
        cfg.integrations.aladin_ttbkey_env = Some("OXIBUILDER_ALADIN_TTBKEY".into());
        cfg.mounts.push(MountConfig {
            id: "docs".into(),
            source: std::path::PathBuf::from("/tmp/docs"),
            path: "/docs".into(),
            title_ko: "문서".into(),
            title_en: "Docs".into(),
            description: Some("Documentation".into()),
            icon: Some("book".into()),
            open_in_new_tab: false,
            hidden: false,
            raw: false,
        });

        let server = cfg.server.clone();
        let settings = MutableSiteSettings::from_config(&cfg);
        let rebuilt = settings.to_config(&server);

        // Mutable fields are preserved verbatim.
        assert_eq!(rebuilt.site.name, "Round");
        assert_eq!(rebuilt.site.base_url, "https://example.test");
        assert_eq!(rebuilt.site.default_lang, "en");
        assert_eq!(rebuilt.site.languages, vec!["en", "ko"]);
        assert_eq!(rebuilt.lobby.default_mode, "list");
        assert_eq!(rebuilt.lobby.layout, "editorial");
        assert_eq!(rebuilt.extensions.enabled, vec!["blog"]);
        assert_eq!(
            rebuilt.integrations.github_username.as_deref(),
            Some("octocat")
        );
        assert_eq!(
            rebuilt.integrations.tmdb_api_key_env.as_deref(),
            Some("OXIBUILDER_TMDB_KEY")
        );
        assert_eq!(
            rebuilt.integrations.aladin_ttbkey_env.as_deref(),
            Some("OXIBUILDER_ALADIN_TTBKEY")
        );
        // Configured mounts survive the settings round trip. Compare field
        // by field because `MountConfig` does not (yet) derive PartialEq.
        assert_eq!(rebuilt.mounts.len(), cfg.mounts.len());
        for (got, want) in rebuilt.mounts.iter().zip(cfg.mounts.iter()) {
            assert_eq!(got.id, want.id);
            assert_eq!(got.source, want.source);
            assert_eq!(got.path, want.path);
            assert_eq!(got.title_ko, want.title_ko);
            assert_eq!(got.title_en, want.title_en);
            assert_eq!(got.description, want.description);
            assert_eq!(got.icon, want.icon);
            assert_eq!(got.open_in_new_tab, want.open_in_new_tab);
        }
        // Server section is passed through unchanged.
        assert_eq!(rebuilt.server.host, cfg.server.host);
        assert_eq!(rebuilt.server.port, cfg.server.port);
    }

    #[test]
    fn deploy_target_round_trips() {
        let mut cfg = Config::default();
        cfg.deploy.github_pages = Some(GitHubPagesTarget {
            owner: "a7garden".into(),
            repo: "notes".into(),
            branch: "pages/v1".into(),
        });
        let settings = MutableSiteSettings::from_config(&cfg);
        assert_eq!(settings.deploy.github_pages, cfg.deploy.github_pages);
        let rebuilt = settings.to_config(&cfg.server);
        assert_eq!(rebuilt.deploy.github_pages, cfg.deploy.github_pages);
    }
}
