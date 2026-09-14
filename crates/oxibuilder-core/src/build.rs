//! Parallel build pipeline for static site generation (v2).
//!
//! Uses rayon to process all extension builders concurrently.
//! Each extension independently produces pages, data, and search docs.

use std::collections::BTreeSet;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use crate::builder::{BuildExt, BuildOutput, ExtBuildOutput};
use crate::media::{ImageManifest, optimize, optimize_external};
use sqlx::SqlitePool;

/// Run all extension builders in parallel via rayon.
///
/// Each extension produces pages, data, and search docs independently.
/// Errors are collected per-extension and reported with context.
///
/// Extensions listed in `inactive` are SKIPPED entirely (no pages, data, or
/// search docs) — the same gate the live route dispatcher applies. Callers
/// resolve it with [`crate::manifest::inactive_extension_ids`] while still on
/// an async context; this fn is fully sync (rayon threads must not `block_on`
/// from a runtime worker). Before this gate existed, disabled extensions
/// still emitted static output that silently collided with static mounts
/// grafted over the same paths (e.g. legacy `/movies` pages mixed with
/// native `movies/{slug}` shells).
pub fn build_site(
    db: &SqlitePool,
    builders: &[Box<dyn BuildExt>],
    inactive: &std::collections::HashSet<String>,
) -> Result<BuildOutput, Box<dyn Error + Send + Sync>> {
    use rayon::prelude::*;
    // Capture the Tokio runtime handle ONCE here, on the runtime thread (this fn is
    // called from the async `build` command). Rayon worker threads have no runtime
    // bound, so `Handle::current()` inside the closure would panic. Passing the
    // captured handle lets each builder `block_on` its async DB work from any thread.
    let rt = tokio::runtime::Handle::current();


    let results: Vec<Result<Option<ExtBuildOutput>, String>> = builders
        .par_iter()
        .map(|ext| {
            let ext_id = ext.ext_id();
            if inactive.contains(ext_id) {
                return Ok(None);
            }
            let pages = ext
                .build_pages(db, &rt)
                .map_err(|e| format!("[{}] build_pages: {}", ext_id, e))?;
            let data = ext
                .build_data(db, &rt)
                .map_err(|e| format!("[{}] build_data: {}", ext_id, e))?;
            let search_docs = ext
                .build_search_docs(db, &rt)
                .map_err(|e| format!("[{}] build_search_docs: {}", ext_id, e))?;
            Ok(Some(ExtBuildOutput {
                ext_id: ext_id.to_string(),
                pages,
                data,
                search_docs,
            }))
        })
        .collect();

    // Collect errors or merge outputs
    let mut outputs = Vec::with_capacity(results.len());
    let mut errors = Vec::new();

    for result in results {
        match result {
            Ok(Some(output)) => outputs.push(output),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        return Err(format!("Build errors:\n{}", errors.join("\n")).into());
    }

    Ok(BuildOutput::merge(outputs))
}

/// Progress events emitted during a streaming build. Relayed to SSE clients
/// and to the build-log recorder. Serialized as `{"event":"<variant>", ...}`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum BuildEvent {
    BuildStarted { total: usize },
    ExtensionStart { ext_id: String },
    ExtensionDone { ext_id: String, pages: usize },
    BuildComplete { total_pages: usize },
    BuildFailed { error: String },
}

/// Streaming variant of [`build_site`].
///
/// Identical build semantics (rayon `par_iter`, per-extension error collection),
/// but emits [`BuildEvent`]s to `tx` as each extension starts/finishes and on
/// completion/failure.
///
/// Takes the Tokio runtime `Handle` explicitly because the console runs this
/// inside `spawn_blocking`, where `Handle::current()` would panic (blocking
/// threads carry no runtime context). `mpsc::Sender::blocking_send` is safe
/// from rayon/spawn_blocking threads (not an async execution context).
pub fn build_site_with_progress(
    db: &SqlitePool,
    builders: &[Box<dyn BuildExt>],
    rt: &tokio::runtime::Handle,
    tx: &tokio::sync::mpsc::Sender<BuildEvent>,
    inactive: &std::collections::HashSet<String>,
) -> Result<BuildOutput, Box<dyn Error + Send + Sync>> {
    use rayon::prelude::*;

    let total = builders.len();
    let _ = tx.blocking_send(BuildEvent::BuildStarted { total });


    let results: Vec<Result<Option<ExtBuildOutput>, String>> = builders
        .par_iter()
        .map(|ext| {
            let ext_id = ext.ext_id();
            let _ = tx.blocking_send(BuildEvent::ExtensionStart {
                ext_id: ext_id.to_string(),
            });
            if inactive.contains(ext_id) {
                // Keep Start/Done pairing so client progress accounting
                // (done/total) stays consistent; a skipped ext contributes 0 pages.
                let _ = tx.blocking_send(BuildEvent::ExtensionDone {
                    ext_id: ext_id.to_string(),
                    pages: 0,
                });
                return Ok(None);
            }
            let pages = ext
                .build_pages(db, rt)
                .map_err(|e| format!("[{}] build_pages: {}", ext_id, e))?;
            let page_count = pages.len();
            let data = ext
                .build_data(db, rt)
                .map_err(|e| format!("[{}] build_data: {}", ext_id, e))?;
            let search_docs = ext
                .build_search_docs(db, rt)
                .map_err(|e| format!("[{}] build_search_docs: {}", ext_id, e))?;
            let _ = tx.blocking_send(BuildEvent::ExtensionDone {
                ext_id: ext_id.to_string(),
                pages: page_count,
            });
            Ok(Some(ExtBuildOutput {
                ext_id: ext_id.to_string(),
                pages,
                data,
                search_docs,
            }))
        })
        .collect();

    let mut outputs = Vec::with_capacity(results.len());
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(Some(output)) => outputs.push(output),
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        let error = format!("Build errors:\n{}", errors.join("\n"));
        let _ = tx.blocking_send(BuildEvent::BuildFailed {
            error: error.clone(),
        });
        return Err(error.into());
    }

    let merged = BuildOutput::merge(outputs);
    let _ = tx.blocking_send(BuildEvent::BuildComplete {
        total_pages: merged.pages.len(),
    });
    Ok(merged)
}

// --- image pre-pass (Task 5 + Task 3) ----------------------------------------
//
// Build-time image pre-pass: scan published blog bodies for `media/...` refs AND
// collect `external_image_urls` from each extension, decode + resize + WebP-encode
// everything to a STAGING directory OUTSIDE `out/`. The staging dir survives the
// `write_build_output` wipe and is copied into `out/media/_derived/` after the
// wipe, alongside `out/data/image-manifest.json` which the static-mode SPA
// plugin (Task 6) reads.
//
// This function does NOT touch the blog builder. Callers (CLI `build` +
// console `ensure_build_started`) own the blog builder separately so they can
// call `BlogExtension::set_manifest` BEFORE boxing it into the builders vec
// — both crates re-create a fresh `Vec<Box<dyn BuildExt>>` per build (cheap,
// and lets the manifest stay scoped to this build only).
//
// Task 3 added the `builders` + `rt` parameters. `rt` is the current Tokio
// runtime handle — required because `BuildExt::external_image_urls` is a SYNC
// trait method that internally calls `rt.block_on` for its async DB query
// (movies lib.rs:473, books lib.rs:303). The collection loop is wrapped in
// `tokio::task::block_in_place` so the nested `block_on` runs off the async
// worker (without it the call panics with "Cannot start a runtime from
// within a runtime" — see ssg_build.rs:433-442 for the documented panic).

/// Scan the published `blog_post` bodies for `media/...` refs and merge each
/// extension's `external_image_urls`, then run `media::optimize` and
/// `media::optimize_external` against `<data_dir>/.image-build/`. Returns
/// `(staging_dir, manifest)`; both are `None` only when the blog table is
/// missing AND every extension returned no external URLs.
///
/// `staging_dir` MUST live outside `out/` — `write_build_output` will wipe
/// `out/` and then re-materialize the derived tree from staging. The canonical
/// staging path is `<data_dir>/.image-build/`.
///
/// Errors are non-fatal at the build level: a missing `blog_post` table
/// (extensions that don't ship migrations to a particular test schema) is
/// treated as "no refs" and falls through to the external-URL branch. Any
/// other DB error propagates as `Err`.
#[allow(clippy::too_many_arguments)]
pub async fn run_image_pre_pass(
    db: &SqlitePool,
    media_dir: &Path,
    data_dir: &Path,
    builders: &[Box<dyn BuildExt>],
    rt: &tokio::runtime::Handle,
) -> io::Result<(Option<PathBuf>, Option<ImageManifest>)> {
    // Read every published blog post's body once. We do NOT decode the markdown
    // — a plain substring scan is enough to enumerate `media/...` refs.
    // `published_at IS NOT NULL` matches the same filter
    // `oxibuilder_ext_blog::repo::list(draft=false, ...)` uses for the build.
    let bodies: Vec<String> = match sqlx::query_as::<_, (String,)>(
        "SELECT body FROM blog_post WHERE published_at IS NOT NULL",
    )
    .fetch_all(db)
    .await
    {
        Ok(rows) => rows.into_iter().map(|(b,)| b).collect(),
        Err(sqlx::Error::Database(dbe)) if dbe.message().contains("no such table") => {
            // Fresh install / test schema without blog_post: nothing to optimize
            // locally, but external URLs from extensions may still exist — fall
            // through and let the external branch handle them.
            Vec::new()
        }
        Err(e) => return Err(io::Error::other(format!("scan blog bodies: {e}"))),
    };

    let refs = collect_media_refs(&bodies);
    let inactive = crate::manifest::inactive_extension_ids(db).await;

    // Collect external image URLs from every extension. The hook is SYNC but
    // internally calls `rt.block_on(async { ... })` (movies lib.rs:473, books
    // lib.rs:303) for its DB query. Calling `Handle::block_on` from inside an
    // async task running on a tokio worker panics with "Cannot start a
    // runtime from within a runtime" — the same panic ssg_build.rs:433-442
    // documents for `build_site`. Wrap the loop in `block_in_place` so the
    // inner `block_on` runs on a blocking thread OUTSIDE the async runtime's
    // TLS context. `block_in_place` requires a multi-threaded runtime; the
    // CLI/console paths both already use one (and the existing tests use
    // `new_multi_thread()` per the pattern at ssg_build.rs:443).
    // `optimize_external(...).await?` stays OUTSIDE the block — it's genuinely
    // async and must remain on the async worker.
    let external: Vec<String> = tokio::task::block_in_place(|| {
        let mut out: Vec<String> = Vec::new();
        for b in builders {
            if inactive.contains(b.ext_id()) {
                continue;
            }
            match b.external_image_urls(db, rt) {
                Ok(urls) => out.extend(urls),
                Err(e) => {
                    tracing::warn!(
                        ext = b.ext_id(),
                        error = %e,
                        "external_image_urls failed, skipping"
                    );
                }
            }
        }
        out
    });

    // Early-return ONLY when both sources are empty — an all-external build
    // (no blog refs) still needs to materialize a staging dir + manifest so
    // the build pipeline can copy variants into `out/media/_derived/`.
    if refs.is_empty() && external.is_empty() {
        return Ok((None, None));
    }

    // Staging lives at `<data_dir>/.image-build/` — well outside `out/`, so
    // `write_build_output`'s `remove_dir_all(out_dir)` doesn't kill our work.
    // The `.image-build` prefix marks it as build-only (analogous to a
    // build cache) so a future deploy step can choose to .gitignore it.
    let staging_dir = data_dir.join(".image-build");
    std::fs::create_dir_all(&staging_dir)?;
    let mut manifest = if refs.is_empty() {
        ImageManifest::empty()
    } else {
        optimize(&refs, media_dir, &staging_dir)?
    };
    if !external.is_empty() {
        // Mirror the movies TMDB client discipline: a tight 5s connect + 15s
        // total ceiling so a stalled CDN can't wedge the build. `unwrap_or_else`
        // falls back to a default client if the builder ever rejects the
        // timeouts (it currently doesn't, but the `?` would lose the build).
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let ext_manifest = optimize_external(&external, &staging_dir, &http).await?;
        for (k, v) in ext_manifest.entries {
            manifest.entries.insert(k, v);
        }
    }
    Ok((Some(staging_dir), Some(manifest)))
}

/// Walk every body, picking up `/?media/...` references (markdown image
/// destinations, raw HTML `src=`/`href=` values, plain text URLs) until the
/// first whitespace, `)`, `"`, `>`, `]`, or `>`. Manual scanner — the
/// `regex` crate isn't in the dep tree, and a 5-line state machine is
/// easier to audit than a regex added just for this.
///
/// Returns sorted, deduplicated refs with any leading `/` already stripped
/// (matches the form `media::optimize` expects).
fn collect_media_refs(bodies: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for body in bodies {
        let bytes = body.as_bytes();
        let mut i = 0;
        while i + 7 <= bytes.len() {
            // Try to match `/media/` or `media/` starting at `i`.
            let (consumed, prefix) = if bytes[i..].starts_with(b"/media/") {
                (1usize, 1usize)
            } else if bytes[i..].starts_with(b"media/") {
                (0usize, 0usize)
            } else {
                i += 1;
                continue;
            };
            let mut j = i + 6 + prefix; // start of the path after `/media/` or `media/`
            while j < bytes.len() {
                let b = bytes[j];
                if b == b' '
                    || b == b'\t'
                    || b == b'\n'
                    || b == b'\r'
                    || b == b')'
                    || b == b'"'
                    || b == b'\''
                    || b == b'>'
                    || b == b']'
                    || b == b'<'
                    || b == b'`'
                {
                    break;
                }
                j += 1;
            }
            if j > i + 6 + prefix {
                let raw = &body[i + consumed..j];
                out.insert(raw.to_string());
            }
            i = j;
        }
    }
    out.into_iter().collect()
}
