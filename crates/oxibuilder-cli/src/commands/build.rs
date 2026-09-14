use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct BuildCommand {
    /// Output directory (default: data/out)
    #[arg(long)]
    pub out_dir: Option<String>,
}

pub(crate) async fn build(c: BuildCommand) -> anyhow::Result<()> {
    let BuildCommand {
        out_dir: custom_out,
    } = c;

    // 1. Resolve data directory from config
    let data_dir = super::resolve_data_dir()?;
    let db_path = data_dir.join("oxibuilder.db");
    let media_dir = data_dir.join("media");

    if !db_path.exists() {
        anyhow::bail!(
            "Database not found at {}. Is the server initialized?",
            db_path.display()
        );
    }

    // 2. Connect to the database
    println!("Building site...");
    println!("  db:      {}", db_path.display());
    println!("  media:   {}", media_dir.display());

    let pool = oxibuilder_core::db::connect(&db_path).await?;

    // 3. Run the image pre-pass BEFORE `build_site`. It scans published
    //    blog bodies for `media/...` refs, decodes + WebP-encodes them into
    //    `<data_dir>/.image-build/` (OUTSIDE `out/` so the writer's wipe
    //    doesn't kill the derived files), and returns the staging dir +
    //    manifest. We then pass the manifest into
    //    `all_builders_with_image_manifest` so the SAME BlogExtension
    //    instance the build_site vec holds sees it (its `set_manifest` is
    //    idempotent — `OnceLock::set` first-call-wins).
    //
    //    The pre-pass also collects `external_image_urls` from every extension
    //    so CDN-hosted posters/covers get the same WebP + manifest treatment
    //    (Task 3). We pass `pre_builders` here with NO manifest yet — the
    //    pre-pass doesn't need it, and the SAME BlogExtension instance is
    //    later re-boxed below with the populated manifest for the build vec.
    let pre_builders: Vec<Box<dyn oxibuilder_core::builder::BuildExt>> =
        oxibuilder_console::all_builders_with_image_manifest(None);
    let pre_pass_rt = tokio::runtime::Handle::current();
    let (image_staging_dir, image_manifest) = oxibuilder_core::build::run_image_pre_pass(
        &pool,
        &media_dir,
        &data_dir,
        &pre_builders,
        &pre_pass_rt,
    )
    .await
    .map_err(|e| anyhow::anyhow!("image pre-pass: {e}"))?;

    let builder_refs: Vec<Box<dyn oxibuilder_core::builder::BuildExt>> =
        oxibuilder_console::all_builders_with_image_manifest(image_manifest.as_ref());
    // Resolve the extension gate ONCE here (async context) — build_site is sync.
    let inactive = oxibuilder_core::manifest::inactive_extension_ids(&pool).await;
    let output = oxibuilder_core::build::build_site(&pool, &builder_refs, &inactive)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    // 5. Write output (sources SPA bundle from the embedded binary, not CWD).
    //    BuildInputs carries site.base_url (drives deployment_base) + theme_id
    //    + the image staging dir + manifest copied after the out/ wipe.

    let out_path = custom_out
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("out"));
    let config_path = std::env::var("OXIBUILDER_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("oxibuilder.toml"));
    let config = if config_path.exists() {
        oxibuilder_core::config::Config::load(&config_path)?
    } else {
        oxibuilder_core::config::Config::default()
    };
    let theme_id = oxibuilder_core::theme::active_theme_id(&pool).await;
    let layout_id = oxibuilder_core::theme::active_layout_id(&pool, &config.lobby.layout).await;
    let mut inputs = oxibuilder_core::builder::BuildInputs::new(
        &config.site.base_url,
        theme_id,
        layout_id,
        "oxibuilder",
    );
    inputs.mounts = config
        .mounts
        .iter()
        .map(oxibuilder_core::builder::MountCopy::from_config)
        .collect();
    // Hand the pre-pass's staging dir + manifest to the writer so it copies
    // the derived WebP variants and emits `out/data/image-manifest.json`
    // after the out/ wipe at step 1 of `write_build_output`.
    inputs.image_staging_dir = image_staging_dir;
    inputs.image_manifest = image_manifest;
    oxibuilder_core::build_writer::write_build_output(&output, &out_path, &media_dir, &inputs)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Emit the lobby manifest as static JSON so `fetchManifest()` resolves in static mode
    // (`pathToStaticFile('/lobby/manifest')` → `/data/lobby.json`). Uses the same assembly as
    // the live `/api/console/lobby/manifest` handler — one shape, no drift.
    // (config already loaded above for BuildInputs; reused here.)
    let extensions = oxibuilder_console::all_extensions();
    let manifest = oxibuilder_core::manifest::assemble(
        &pool,
        &config,
        &config.site.name,
        &config.site.base_url,
        &extensions,
    )
    .await;
    std::fs::write(
        out_path.join("data").join("lobby.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    println!("Build complete:");
    println!("  pages:     {}", output.pages.len());
    println!("  search:    {} docs", output.search_docs.len());
    println!("  output:    {}", out_path.display());
    println!();
    println!("  Preview:  oxibuilder console --preview");
    println!("  Deploy:   oxibuilder deploy --target github-pages");

    Ok(())
}
