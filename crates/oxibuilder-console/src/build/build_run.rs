//! Build-run orchestration on top of the shared [`SiteOperationGuard`].
//!
//! The guard owns the per-site operation slot (conflict detection, broadcast
//! fan-out, retained terminal state) and the start-CAS. This module provides
//! the lazy-start entry point that turns the guard's slot into an actual SSG
//! build: claim the slot, spawn the blocking build, relay every [`BuildEvent`]
//! as an [`OperationEvent`], publish a terminal event, record the log row,
//! and release the slot.

use crate::operations::{OperationEvent, SiteOperationGuard, SiteOperationKind};
use crate::sites_runtime::SiteContext;
use oxibuilder_core::build::BuildEvent;
use oxibuilder_core::builder::BuildInputs;
use sqlx::SqlitePool;
use std::sync::Arc;

/// Start the in-flight build for `slug` exactly once (CAS via the guard).
/// The first caller — an SSE subscriber or the 3s watchdog — wins and spawns
/// the build; later callers are no-ops. Returns `false` if no build is
/// registered for `slug` or another caller already owns starting it.
///
/// Must be called from a Tokio runtime context (captures `Handle::current()`
/// for the `spawn_blocking` build).
pub async fn ensure_build_started(
    guard: &Arc<SiteOperationGuard>,
    ctx: &Arc<SiteContext>,
    build_id: &str,
) -> bool {
    // Only the registered run may be started.
    let Some(snapshot) = guard.current(&ctx.slug) else {
        return false;
    };
    if snapshot.run_id != build_id {
        return false;
    }
    if !guard.try_claim(&ctx.slug) {
        return true; // already started by another caller
    }

    let slug = ctx.slug.clone();
    let db = ctx.db.clone();

    let out_dir = ctx.out_dir.clone();
    let media_dir = ctx.media_dir.clone();
    let data_dir = ctx.data_dir.clone();
    let started_at = snapshot.started_at;
    let site_base_url = ctx.settings.read().await.site.base_url.clone();
    let theme_id = oxibuilder_core::theme::active_theme_id(&ctx.db).await;
    let layout_default =
        oxibuilder_core::config::Config::load(&ctx.project_dir.join("oxibuilder.toml"))
            .map(|config| config.lobby.layout)
            .unwrap_or_else(|_| "shell".to_string());
    let layout_id = oxibuilder_core::theme::active_layout_id(&ctx.db, &layout_default).await;
    let mounts = ctx.settings.read().await.mounts.clone();
    let guard = guard.clone();
    tokio::spawn(async move {
        let rt = tokio::runtime::Handle::current();
        let (mpsc_tx, mut mpsc_rx) = tokio::sync::mpsc::channel::<BuildEvent>(64);

        // Relay build events (sync, from the build task) → OperationEvent
        // broadcast (async, to SSE subscribers).
        let relay_slug = slug.clone();
        let relay_guard = guard.clone();
        tokio::spawn(async move {
            while let Some(ev) = mpsc_rx.recv().await {
                let terminal = matches!(
                    ev,
                    BuildEvent::BuildComplete { .. } | BuildEvent::BuildFailed { .. }
                );
                let _ = relay_guard.publish(
                    &relay_slug,
                    OperationEvent {
                        event: event_name(&ev).to_string(),
                        data: serde_json::to_value(&ev).unwrap_or(serde_json::Value::Null),
                        terminal,
                    },
                );
            }
        });

        // Image pre-pass runs BEFORE build_site_with_progress so BlogExtension's
        // manifest is populated when build_pages fires. We pass the manifest
        // into `all_builders_with_image_manifest` so the SAME BlogExtension
        // instance the build_site vec holds sees it (its `set_manifest` is
        // idempotent — `OnceLock::set` first-call-wins). The fresh vec is
        // created per build (cheap) so the manifest stays scoped to this
        // build only; the `Arc<Vec<…>>` in `SiteContext.builders` is read-only
        // here and would block the cast.
        //
        // The pre-pass also collects `external_image_urls` from every extension
        // (Task 3) so CDN-hosted posters/covers get the same WebP + manifest
        // treatment. We pass a `pre_builders` slice built WITHOUT a manifest —
        // the pre-pass doesn't need one and re-boxes a fresh vec below with
        // the populated manifest for `build_site_with_progress`. `rt` is the
        // captured runtime handle (already a Handle on this async task).
        let pre_builders_vec: Vec<Box<dyn oxibuilder_core::builder::BuildExt>> =
            crate::all_builders_with_image_manifest(None);
        let pre_pass_outcome: Result<
            (
                Option<std::path::PathBuf>,
                Option<oxibuilder_core::media::ImageManifest>,
            ),
            String,
        > = oxibuilder_core::build::run_image_pre_pass(
            &db,
            &media_dir,
            &data_dir,
            &pre_builders_vec,
            &rt,
        )
        .await
        .map_err(|e| format!("image pre-pass: {e}"));
        let outcome: Result<usize, String> = match pre_pass_outcome {
            Err(e) => Err(e),
            Ok((staging, manifest)) => {
                // Build the per-build builder vec with the manifest already
                // pushed into the SAME BlogExtension instance we'll dispatch.
                let builders_vec: Vec<Box<dyn oxibuilder_core::builder::BuildExt>> =
                    crate::all_builders_with_image_manifest(manifest.as_ref());
                // Resolve the extension gate ONCE here (async context) — the
                // build below runs on a spawn_blocking thread.
                let inactive = oxibuilder_core::manifest::inactive_extension_ids(&db).await;
                let db_task = db.clone();
                let out_task = out_dir.clone();
                let media_task = media_dir.clone();
                let base_url_task = site_base_url.clone();
                let theme_task = theme_id.clone();
                let layout_task = layout_id.clone();
                let mounts_task = mounts.clone();
                tokio::task::spawn_blocking(move || {
                    match oxibuilder_core::build::build_site_with_progress(
                        &db_task,
                        &builders_vec,
                        &rt,
                        &mpsc_tx,
                        &inactive,
                    ) {
                        Ok(output) => {
                            let mut inputs = BuildInputs::new(
                                &base_url_task,
                                &theme_task,
                                &layout_task,
                                "oxibuilder",
                            );
                            inputs.image_staging_dir = staging;
                            inputs.image_manifest = manifest;
                            inputs.mounts = mounts_task
                                .iter()
                                .map(oxibuilder_core::builder::MountCopy::from_config)
                                .collect();
                            if let Err(e) = oxibuilder_core::build_writer::write_build_output(
                                &output,
                                &out_task,
                                &media_task,
                                &inputs,
                            ) {
                                return Err(format!("write_build_output: {e}"));
                            }
                            Ok(output.pages.len())
                        }
                        Err(e) => Err(e.to_string()),
                    }
                })
                .await
                .unwrap_or_else(|e| Err(format!("build task panicked: {e}")))
            }
        };

        record_build_log(&db, &started_at, &out_dir, &outcome).await;

        // Publish a terminal snapshot before releasing the slot so a
        // reconnecting client sees the final state.
        match &outcome {
            Ok(pages) => {
                let _ = guard.publish(
                    &slug,
                    OperationEvent::terminal(
                        "build_complete",
                        serde_json::json!({ "total_pages": pages }),
                    ),
                );
            }
            Err(e) => {
                let _ = guard.publish(
                    &slug,
                    OperationEvent::terminal("build_failed", serde_json::json!({ "error": e })),
                );
            }
        }
        let _ = guard.finish(&slug);
    });
    true
}

fn event_name(ev: &BuildEvent) -> &'static str {
    match ev {
        BuildEvent::BuildStarted { .. } => "build_started",
        BuildEvent::ExtensionStart { .. } => "extension_start",
        BuildEvent::ExtensionDone { .. } => "extension_done",
        BuildEvent::BuildComplete { .. } => "build_complete",
        BuildEvent::BuildFailed { .. } => "build_failed",
    }
}

/// Record a finished build in the per-site `build_log` table. Idempotent schema
/// setup (`finished_at`/`error` columns added if missing — ALTER errors ignored).
async fn record_build_log(
    db: &SqlitePool,
    started_at: &str,
    out_dir: &std::path::Path,
    outcome: &Result<usize, String>,
) {
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS build_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            status TEXT NOT NULL DEFAULT 'built',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            page_count INTEGER,
            out_dir TEXT
        )",
    )
    .execute(db)
    .await;
    let _ = sqlx::query("ALTER TABLE build_log ADD COLUMN finished_at TEXT")
        .execute(db)
        .await;
    let _ = sqlx::query("ALTER TABLE build_log ADD COLUMN error TEXT")
        .execute(db)
        .await;

    let (status, page_count, error): (&str, Option<i64>, Option<&str>) = match outcome {
        Ok(n) => ("built", Some(*n as i64), None),
        Err(e) => ("failed", None, Some(e.as_str())),
    };
    let _ = sqlx::query(
        "INSERT INTO build_log (status, page_count, out_dir, created_at, finished_at, error)
         VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5)",
    )
    .bind(status)
    .bind(page_count)
    .bind(out_dir.to_string_lossy().to_string())
    .bind(started_at)
    .bind(error)
    .execute(db)
    .await;
}

/// Site operation kind used when registering a build slot.
pub fn build_operation_kind() -> SiteOperationKind {
    SiteOperationKind::Build
}
