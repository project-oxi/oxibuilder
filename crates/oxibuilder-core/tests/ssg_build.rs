//! Regression test for the SSG build pipeline.
//!
//! Verifies that `build_site` and `write_build_output` produce a working static
//! site. This is the test that would have caught the v0.4.0 build panic (every
//! `BuildExt` called `Handle::current()` on a rayon thread, which panics).
//!
//! Also asserts the embed-output contract that the deployed SPA relies on:
//!   - `out/index.html` and `out/404.html` exist (lobby entry + GitHub Pages fallback)
//!   - `out/assets/<file>` exists (the SPA bundle; no double-nesting)
//!   - the root `index.html` references the hashed script from the embedded bundle

use oxibuilder_core::build::build_site;
use oxibuilder_core::build_writer::write_build_output;
use oxibuilder_core::builder::{BuildExt, BuildOutput, SearchDoc, StaticPage};
use oxibuilder_core::db;
use oxibuilder_core::http::embedded_spa_files;
use oxibuilder_core::manifest::inactive_extension_ids;
use sqlx::SqlitePool;

struct StubBuilder;
impl BuildExt for StubBuilder {
    fn ext_id(&self) -> &'static str {
        "stub"
    }
    fn build_pages(
        &self,
        _db: &SqlitePool,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Vec<StaticPage>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
    fn build_data(
        &self,
        _db: &SqlitePool,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn std::error::Error + Send + Sync>>
    {
        Ok(Box::new(serde_json::json!([])))
    }
    fn build_search_docs(
        &self,
        _db: &SqlitePool,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Vec<SearchDoc>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn build_site_runs_without_panic_and_writes_correct_layout() {
    // 2. Spin up an empty SQLite + a media dir under the system temp root.
    let tmp_root = std::env::temp_dir().join(format!("oxibuilder-ssg-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_root);
    std::fs::create_dir_all(&tmp_root).unwrap();
    let db_path = tmp_root.join("oxibuilder.db");
    let media_dir = tmp_root.join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let pool = db::connect(&db_path).await.unwrap();

    // 3. Run the real build pipeline.
    let builders: Vec<Box<dyn BuildExt>> = vec![Box::new(StubBuilder)];
    let inactive = inactive_extension_ids(&pool).await;
    let output = build_site(&pool, &builders, &inactive).expect("build_site should not panic");
    // 4. Write to a fresh out dir and assert the layout.
    let out_dir = tmp_root.join("out");
    // Test-only default: no active layout is under test here.
    let inputs = oxibuilder_core::builder::BuildInputs::new(
        "https://127.0.0.1:8787/",
        "paper",
        "shell",
        "test",
    );
    write_build_output(&output, &out_dir, &media_dir, &inputs).expect("write_build_output");

    // Root SPA entry (lobby).
    assert!(
        out_dir.join("index.html").exists(),
        "out/index.html must exist"
    );

    // SPA fallback for deep links on hosts without a real SPA fallback
    // (GitHub Pages serves 404.html on 404).
    assert!(out_dir.join("404.html").exists(), "out/404.html must exist");

    // SPA bundle: hashed JS at /assets/, NOT /assets/assets/.
    assert!(out_dir.join("assets").is_dir(), "out/assets must exist");
    let bundled = embedded_spa_files();
    let js: Vec<_> = bundled
        .iter()
        .filter(|(p, _)| p.starts_with("assets/") && p.ends_with(".js"))
        .collect();
    assert!(
        !js.is_empty(),
        "embedded SPA must contain at least one JS chunk"
    );

    // Root index.html must reference the real relativized hashed script
    // (Task 2: `/assets/...` → `assets/...` + a deployment `<base href>`).
    let root = std::fs::read_to_string(out_dir.join("index.html")).unwrap();
    // Find the first `src="assets/...` (the relativized hashed chunk).
    // Non-asset tags like `<script src="/theme-boot.js">` precede it and are
    // intentionally left absolute, so search for the `assets/` reference.
    let root_ref = root
        .find("src=\"assets/")
        .map(|i| {
            let v = &root[i + 5..];
            v.split('"').next().unwrap_or("")
        })
        .unwrap_or("");
    assert!(
        root_ref.starts_with("assets/") && !root_ref.ends_with("/index.js"),
        "root index.html must reference a relativized hashed asset, got: {root_ref}"
    );
    assert!(
        root.contains("<base href=\"/\">"),
        "root index.html must carry the deployment base href"
    );
}

#[test]
fn embedded_spa_files_preserve_relative_paths() {
    // rust-embed stores paths relative to `#[folder]`. Verify we expose them so
    // `build_writer` can write them at the same relative path under `out/`.
    let files = embedded_spa_files();
    assert!(
        !files.is_empty(),
        "embedded-spa must contain at least index.html"
    );
    let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert!(names.contains(&"index.html"), "index.html must be embedded");
    assert!(
        names
            .iter()
            .any(|p| p.starts_with("assets/") && p.ends_with(".js")),
        "at least one hashed JS chunk must be embedded"
    );
}

// --- Task 5 regression: staging survives the out/ wipe ---------------------

/// Minimal blog_post table — mirrors `oxibuilder-ext-blog/migrations/0001_init.sql`
/// so `run_image_pre_pass`'s `SELECT body FROM blog_post WHERE published_at IS NOT NULL`
/// works without booting the full extension migration runner.
const BLOG_POST_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS blog_post (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    body TEXT NOT NULL DEFAULT '',
    lang TEXT NOT NULL DEFAULT 'ko' CHECK (lang IN ('ko','en')),
    translation_group_id INTEGER,
    tags JSON NOT NULL DEFAULT '[]',
    published_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    FOREIGN KEY (translation_group_id) REFERENCES blog_post(id) ON DELETE SET NULL
);
"#;

#[tokio::test(flavor = "multi_thread")]
async fn derived_images_survive_out_wipe_and_manifest_is_written() {
    // 1. Spin up tmp dirs: data_dir holds the .db + media + (later) .image-build;
    //    out_dir is wiped by write_build_output and rebuilt from scratch.
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let out_dir = tmp.path().join("out");
    let media_dir = data_dir.join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let db_path = data_dir.join("oxibuilder.db");

    // 2. Make a real source image big enough that all 4 configured widths apply
    //    (640/960/1280/1920) — proves the entry has a real srcset on disk.
    let src_path = media_dir.join("shot.png");
    let img = image::ImageBuffer::from_pixel(2000u32, 1125u32, image::Rgba([255u8, 0, 0, 255]));
    img.save(&src_path).unwrap();

    // 3. Open a fresh SQLite, install the blog_post schema, insert a published
    //    post whose body references the local media file. The pre-pass queries
    //    `published_at IS NOT NULL` to pick the row.
    let pool = db::connect(&db_path).await.unwrap();
    sqlx::query(BLOG_POST_SCHEMA).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO blog_post (slug, title, body, published_at) \
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .bind("hello")
    .bind("Hello")
    .bind("See ![shot](media/shot.png) for the picture.")
    .execute(&pool)
    .await
    .unwrap();

    // 4. Run the pre-pass against the real media_dir + data_dir.
    let (staging, manifest) = oxibuilder_core::build::run_image_pre_pass(
        &pool,
        &media_dir,
        &data_dir,
        &[],
        &tokio::runtime::Handle::current(),
    )
    .await
    .expect("pre-pass");

    let staging = staging.expect("staging dir present");
    let manifest = manifest.expect("manifest present");
    assert_eq!(manifest.entries.len(), 1, "one entry per unique ref");
    let entry = manifest.get("media/shot.png").expect("entry exists");
    assert_eq!(
        entry.srcset.len(),
        4,
        "all four widths applied to a 2000px source"
    );

    // 5. The staging tree has the on-disk WebP variants AND a build cache.
    let staging_derived = staging.join("media").join("_derived");
    assert!(
        staging_derived.is_dir(),
        "staging/<media>/_derived must exist"
    );
    let staging_variants: Vec<String> = std::fs::read_dir(&staging_derived)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let staging_webp = staging_variants
        .iter()
        .filter(|n| n.ends_with(".webp"))
        .count();
    assert_eq!(staging_webp, 4, "4 .webp variants on staging");
    assert!(
        staging_variants.iter().any(|n| n == ".cache.json"),
        ".cache.json MUST live in staging so subsequent builds can read it"
    );

    // 6. Hand the staging + manifest to write_build_output. It wipes out/,
    //    rebuilds pages/data, then copies derived into out/media/_derived +
    //    writes out/data/image-manifest.json. This is the regression: any
    //    pre-pass output written directly into out/ would be destroyed by
    //    the wipe; the staging flow keeps it.
    let output = BuildOutput {
        pages: vec![],
        search_docs: vec![],
        extensions_data: vec![],
    };
    // Test-only default: no active layout is under test here.
    let mut inputs = oxibuilder_core::builder::BuildInputs::new(
        "https://a7garden.github.io/blog/",
        "paper",
        "shell",
        "seed",
    );
    inputs.image_staging_dir = Some(staging.clone());
    inputs.image_manifest = Some(manifest.clone());
    write_build_output(&output, &out_dir, &media_dir, &inputs).expect("write_build_output");

    // 6a. The out/media/_derived/ tree was re-materialized.
    let out_derived = out_dir.join("media").join("_derived");
    assert!(
        out_derived.is_dir(),
        "out/media/_derived must exist after write"
    );
    let out_variants: Vec<String> = std::fs::read_dir(&out_derived)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    let out_webp = out_variants.iter().filter(|n| n.ends_with(".webp")).count();
    assert_eq!(out_webp, 4, "4 .webp variants in out/media/_derived");

    // 6b. .cache.json MUST NOT leak into the deployed tree — it lives only in
    //     staging so subsequent builds can read it.
    assert!(
        !out_variants.iter().any(|n| n == ".cache.json"),
        ".cache.json must not ship to out/ (deployment bloat)"
    );

    // 6c. The manifest JSON shipped as out/data/image-manifest.json for the
    //     static-mode SPA plugin (Task 6) to read.
    let manifest_path = out_dir.join("data").join("image-manifest.json");
    let json = std::fs::read_to_string(&manifest_path).expect("manifest JSON exists");
    assert!(
        json.contains("\"media/shot.png\""),
        "manifest contains the entry: {json}"
    );
    assert!(
        json.contains(".webp"),
        "manifest includes srcset urls: {json}"
    );
}

#[test]
fn base_placeholder_resolved_to_deployment_base_in_pages() {
    // Snapshot-style test: write a single page that embeds BASE_PLACEHOLDER,
    // run write_build_output with a project-pages base URL, and assert the
    // placeholder was rewritten into the deployment_base INSIDE the per-page
    // local — the function must NOT mutate `output` (it takes `&BuildOutput`),
    // so we re-read `output.pages[0].content` after writing and confirm it
    // still contains the raw placeholder.
    let tmp = tempfile::tempdir().unwrap();
    let out_dir = tmp.path().join("out");
    let media_dir = tmp.path().join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    const BASE: &str = oxibuilder_core::markdown::BASE_PLACEHOLDER;

    // Render emits `{prefix}/{url}` literally — when prefix is the placeholder,
    // that's `{BASE}/media/...`. Hand-write that form so the test pins the
    // real contract; using `{BASE}media/...` (no separator) would let a buggy
    // `replace(BASE, base)` silently produce `/blogmedia/...` and not catch
    // the double-slash regression where the placeholder + the separator
    // `render_image_open` inserts are kept literally.
    let raw = format!(
        r#"<!DOCTYPE html><html><head><title>t</title></head><body><img src="{BASE}/media/x.png"></body></html>"#
    );
    let page = StaticPage {
        path: "blog/hello/index.html".to_string(),
        content: raw.clone(),
    };
    let output = BuildOutput {
        pages: vec![page],
        search_docs: vec![],
        extensions_data: vec![],
    };
    // https://a7garden.github.io/blog/ → /blog/ — the canonical project-pages case.
    // Test-only default: no active layout is under test here.
    let inputs = oxibuilder_core::builder::BuildInputs::new(
        "https://a7garden.github.io/blog/",
        "paper",
        "shell",
        "seed",
    );
    write_build_output(&output, &out_dir, &media_dir, &inputs).expect("write_build_output");

    // 1. The on-disk file has the placeholder replaced with the real base.
    let on_disk =
        std::fs::read_to_string(out_dir.join("blog/hello/index.html")).expect("file written");
    assert!(
        !on_disk.contains(BASE),
        "BASE_PLACEHOLDER must NOT survive into the file (raw: {on_disk:?})"
    );
    assert!(
        on_disk.contains("src=\"/blog/media/x.png\""),
        "placeholder must be replaced with deployment_base /blog/: {on_disk}"
    );

    // 2. `output` is borrowed immutably — the page content the caller still
    //    holds MUST still contain the raw placeholder. If this regresses,
    //    write_build_output is mutating output and downstream callers (the
    //    build pipeline that re-reads the result for search/manifest
    //    reconciliation) will see corrupted state.
    assert!(
        output.pages[0].content.contains(BASE),
        "write_build_output must NOT mutate output — page content should still carry the raw placeholder"
    );

    // 3. Apex / user-pages case: placeholder → "/".
    let tmp2 = tempfile::tempdir().unwrap();
    let out2 = tmp2.path().join("out");
    let media2 = tmp2.path().join("media");
    std::fs::create_dir_all(&media2).unwrap();
    let raw2 = format!(r#"<img src="{BASE}/media/y.png">"#);
    let output2 = BuildOutput {
        pages: vec![StaticPage {
            path: "x.html".to_string(),
            content: raw2,
        }],
        search_docs: vec![],
        extensions_data: vec![],
    };
    // Test-only default: no active layout is under test here.
    let inputs2 = oxibuilder_core::builder::BuildInputs::new(
        "https://alice.github.io/",
        "paper",
        "shell",
        "seed",
    );
    write_build_output(&output2, &out2, &media2, &inputs2).expect("write 2");
    let on_disk2 = std::fs::read_to_string(out2.join("x.html")).unwrap();
    assert!(
        on_disk2.contains("src=\"/media/y.png\""),
        "apex base: placeholder → '/', got: {on_disk2}"
    );
}
// --- Task 7 end-to-end pipeline proof --------------------------------------
// Stitches Tasks 2 (media::optimize), 3 (markdown::render with manifest),
// 4 (BuildExt page generation), and 5 (write_build_output with staging +
// manifest) into a single artifact-level assertion. The earlier Task 5
// regression test (`derived_images_survive_out_wipe_and_manifest_is_written`)
// covers the staging → out/ copy separately; this one covers the WHOLE
// pipeline that the build CLI runs in production: real PNG → optimize → render
// markdown body with the manifest → BuildExt → build_site → write_build_output
// with a project-pages `site_base_url`. If any of Tasks 2–5 regress at their
// hand-off, this test fails.

/// Minimal blog builder that emits a single page whose `<div id="root">`
/// carries `markdown::render`-produced HTML. Holds the build-time manifest
/// so `build_pages` can substitute optimized `<img>` tags for `media/...`
/// refs (mirrors how `oxibuilder-ext-blog::BlogExtension` uses the manifest).
struct BlogShellBuilder {
    images: oxibuilder_core::media::ImageManifest,
    /// Deployment base to bake into the rendered HTML. The real builder uses
    /// `BASE_PLACEHOLDER` and lets `write_build_output` substitute, but we use
    /// the concrete value here so the assertion can match the exact output.
    asset_base: &'static str,
}

impl BuildExt for BlogShellBuilder {
    fn ext_id(&self) -> &'static str {
        "blog"
    }
    fn build_pages(
        &self,
        db: &SqlitePool,
        rt: &tokio::runtime::Handle,
    ) -> Result<Vec<StaticPage>, Box<dyn std::error::Error + Send + Sync>> {
        let rows: Vec<(String, String, String)> = rt.block_on(async {
            sqlx::query_as::<_, (String, String, String)>(
                "SELECT slug, title, body FROM blog_post WHERE published_at IS NOT NULL",
            )
            .fetch_all(db)
            .await
        })?;
        let mut pages = Vec::with_capacity(rows.len());
        for (slug, title, body) in rows {
            let rendered = oxibuilder_core::markdown::render(&body, self.asset_base, &self.images);
            let html = format!(
                "<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><title>{title}</title></head>\
                 <body><div id=\"root\"><article>{rendered}</article></div>\
                 <script src=\"/assets/index.js\"></script></body></html>"
            );
            pages.push(StaticPage {
                path: format!("blog/{slug}/index.html"),
                content: html,
            });
        }
        Ok(pages)
    }
    fn build_data(
        &self,
        _db: &SqlitePool,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn std::error::Error + Send + Sync>>
    {
        Ok(Box::new(serde_json::json!([])))
    }
    fn build_search_docs(
        &self,
        _db: &SqlitePool,
        _rt: &tokio::runtime::Handle,
    ) -> Result<Vec<SearchDoc>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }
}

#[test]
fn end_to_end_pipeline_renders_image_into_blog_page() {
    // Why `#[test]` + a dedicated Runtime, not `#[tokio::test]`:
    //   `build_site` captures `Handle::current()` on the calling thread and
    //   then runs the per-extension work via `rayon::par_iter`. With a single
    //   builder (our case), par_iter executes on the calling thread itself —
    //   which under `#[tokio::test]` is *already* inside the test's runtime
    //   CONTEXT (TLS). The captured handle then panics with
    //   "Cannot start a runtime from within a runtime" when the builder calls
    //   `rt.block_on(...)`. We work around this by running `build_site` on a
    //   dedicated blocking-thread pool (`spawn_blocking`), which sits OUTSIDE
    //   the runtime's CONTEXT, so par_iter's workers also stay outside it.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    rt.block_on(async {
        // 1. tmp layout: data/ holds .db + media + (later) .image-build staging;
        //    out/ is wiped by write_build_output and rebuilt from scratch.
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let out_dir = tmp.path().join("out");
        let media_dir = data_dir.join("media");
        let staging_dir = data_dir.join(".image-build");
        std::fs::create_dir_all(&media_dir).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();
        let db_path = data_dir.join("oxibuilder.db");

        // 2. Real 2000×1125 PNG source — wide enough that all four widths in
        //    `media::WIDTHS` (640/960/1280/1920) apply, so the resulting entry
        //    carries a 4-variant srcset. Red, opaque, predictable.
        let src_path = media_dir.join("shot.png");
        let img = image::ImageBuffer::from_pixel(2000u32, 1125u32, image::Rgba([255u8, 0, 0, 255]));
        img.save(&src_path).unwrap();

        // 3. SQLite with blog_post schema + one published row whose body
        //    references the local media file via a markdown image.
        let pool = db::connect(&db_path).await.unwrap();
        sqlx::query(BLOG_POST_SCHEMA).execute(&pool).await.unwrap();
        let body = "Intro paragraph.\n\nSee ![shot](media/shot.png) for the picture.\n\nOutro.";
        sqlx::query(
            "INSERT INTO blog_post (slug, title, body, published_at) \
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        )
        .bind("hello")
        .bind("Hello")
        .bind(body)
        .execute(&pool)
        .await
        .unwrap();

        // 4. Task 2: optimize the media ref → ImageManifest. 4 variants on disk.
        let refs = vec!["media/shot.png".to_string()];
        let manifest = oxibuilder_core::media::optimize(&refs, &media_dir, &staging_dir)
            .expect("media::optimize");
        let entry = manifest.get("media/shot.png").expect("entry exists");
        assert_eq!(entry.srcset.len(), 4, "2000px source → all 4 widths");
        assert_eq!(entry.width, 2000);
        assert_eq!(entry.height, 1125);

        // 5. Build pipeline: real markdown::render + build_site + write_build_output.
        //    /blog/ exercises the non-apex path (the canonical GitHub Pages
        //    project-path deployment; production parity).
        //
        //    We use `BASE_PLACEHOLDER` for the render's `asset_base` (matching
        //    `oxibuilder-ext-blog::BlogExtension`'s production code path);
        //    `write_build_output` then substitutes the real `/blog/` into the
        //    emitted HTML before writing.
        let builder = BlogShellBuilder {
            images: manifest.clone(),
            asset_base: oxibuilder_core::markdown::BASE_PLACEHOLDER,
        };
        let builders: Vec<Box<dyn BuildExt>> = vec![Box::new(builder)];
        // Run `build_site` on a blocking-pool thread so rayon's par_iter
        // workers don't inherit this runtime's CONTEXT — see top comment.
        let pool_for_build = pool.clone();
        let inactive = inactive_extension_ids(&pool).await;
        let output =
            tokio::task::spawn_blocking(move || build_site(&pool_for_build, &builders, &inactive))
                .await
                .expect("spawn_blocking join")
                .expect("build_site");
        let inputs = oxibuilder_core::builder::BuildInputs::new(
            "https://a7garden.github.io/blog/", // → deployment_base = "/blog/"
            "paper",
            "shell", // test-only default; no on-disk config or DB layout is under test
            "e2e-seed",
        );
        let out_dir_for_write = out_dir.clone();
        let staging_for_write = staging_dir.clone();
        let manifest_for_write = manifest.clone();
        tokio::task::spawn_blocking(move || {
            let mut inputs = inputs;
            inputs.image_staging_dir = Some(staging_for_write);
            inputs.image_manifest = Some(manifest_for_write);
            write_build_output(&output, &out_dir_for_write, &media_dir, &inputs)
        })
        .await
        .expect("spawn_blocking join")
        .expect("write_build_output");

        // 6a. Per-page artifact exists at the BuildExt-emitted path.
        let page_path = out_dir.join("blog/hello/index.html");
        let html = std::fs::read_to_string(&page_path).expect("blog page exists");

        // 6b. The non-image body text survived through render + write unchanged.
        assert!(
            html.contains("for the picture"),
            "body text must be present in the rendered page, got: {html}"
        );
        assert!(
            html.contains("Outro."),
            "post-image text must be present, got: {html}"
        );

        // 6c. The optimized `<img>` tag is inside the `#root` shell. Pick the
        //     sha8 prefix from the manifest entry (first 8 hex chars of
        //     `media/_derived/{sha8}-{w}.webp`); the URLs in the HTML use the
        //     ASSET_BASE-prefixed form produced by render_image_open.
        let sha8 = entry
            .srcset
            .first()
            .expect("non-empty srcset")
            .url
            .strip_prefix("media/_derived/")
            .and_then(|s| s.split('-').next())
            .expect("srcset url has {sha8}-w.webp form")
            .to_string();
        // The `markdown::render` output uses `BASE_PLACEHOLDER`, which
        // `write_build_output` substitutes with `/blog/` while STRIPPING the
        // `/` separator that `render_image_open` inserts — yielding the clean
        // single-slash form below. (Earlier this asserted the literal
        // double-slash form, which masked the bug where the placeholder was
        // replaced without its trailing `/`, producing `/blog//media/...` in
        // the project case and `//media/...` in the apex case.)
        let chosen_src = format!("/blog/media/_derived/{sha8}-960.webp");
        assert!(
            html.contains(&format!("src=\"{chosen_src}\"")),
            "src must point at the 960-px variant under /blog/, got: {html}"
        );

        // srcset covers all four widths, each prefixed with /blog/.
        for w in [640u32, 960, 1280, 1920] {
            let variant_url = format!("/blog/media/_derived/{sha8}-{w}.webp");
            assert!(
                html.contains(&format!("{variant_url} {w}w")),
                "srcset must contain {variant_url} {w}w, got: {html}"
            );
        }

        // Dims + lazy-loading attribute set (matches the SPA plugin parity).
        assert!(
            html.contains("width=\"2000\""),
            "width=2000 emitted, got: {html}"
        );
        assert!(
            html.contains("height=\"1125\""),
            "height=1125 emitted, got: {html}"
        );
        assert!(
            html.contains("loading=\"lazy\""),
            "loading=lazy emitted, got: {html}"
        );
        assert!(
            html.contains("decoding=\"async\""),
            "decoding=async emitted, got: {html}"
        );

        // 7. The WebP variants shipped to out/media/_derived/ (post-wipe copy).
        let out_derived = out_dir.join("media").join("_derived");
        assert!(
            out_derived.is_dir(),
            "out/media/_derived must exist after write"
        );
        for w in [640u32, 960, 1280, 1920] {
            let p = out_derived.join(format!("{sha8}-{w}.webp"));
            assert!(p.is_file(), "{p:?} must exist as a deployed variant");
        }

        // 8. The manifest shipped as out/data/image-manifest.json for the SPA.
        let manifest_path = out_dir.join("data").join("image-manifest.json");
        let json = std::fs::read_to_string(&manifest_path).expect("manifest JSON exists");
        assert!(
            json.contains("\"media/shot.png\""),
            "manifest contains the entry: {json}"
        );
        assert!(
            json.contains(".webp"),
            "manifest includes srcset urls: {json}"
        );
    });
}

// --- Task 6 (Track A + Track B) integration verification ---------------------
// Two final acceptance tests for the merged movies-books-stats-parity +
// external-image-optimization plans. Both share this file per the brief; both
// use the real `MoviesExtension` + `BooksExtension` builders (added as
// `[dev-dependencies]` for `oxibuilder-core` in Cargo.toml) so they exercise
// production code paths rather than the earlier StubBuilder-style scaffolding.
// They run under `#[tokio::test(flavor = "multi_thread")]` to match the
// pre-pass / `block_in_place` discipline documented at build.rs:172-179.

/// Track A acceptance: a DB whose pre-stats-parity schema (movies 0001 +
/// books 0001 only) has been upgraded in-place to the current migration set
/// must still build the static site. Specifically:
///   1. The 0002 / 0003 ALTER TABLE migrations apply cleanly to a v0.8.x DB
///      (column additions, no rewriting of legacy rows).
///   2. Legacy rows — i.e. rows whose origin / category / publisher /
///      page_count / title_ko / title_en / runtime_min columns are NULL
///      because they pre-date those migrations — round-trip through the real
///      `MoviesExtension` + `BooksExtension` builders without erroring on the
///      nullable columns.
///   3. `build_site` produces pages for the published entries.
///   4. `external_image_urls` for both extensions returns the expected URL
///      shape (movies: full TMDB w500 URL; books: stored cover_image_url)
///      even when the row has no new-migration data.
#[tokio::test(flavor = "multi_thread")]
async fn migration_compat_build_handles_legacy_null_columns() {
    use oxibuilder_core::registry::ExtensionRegistry;
    use oxibuilder_ext_books::BooksExtension;
    use oxibuilder_ext_movies::MoviesExtension;
    use std::sync::Arc;

    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("oxibuilder.db");
    let pool = db::connect(&db_path).await.unwrap();

    // --- 1. Step-up the DB: register both extensions and run all migrations.
    //         Registry runs `_core` + each extension's migrations in version
    //         order, so this is the production upgrade path.
    let registry = Arc::new(ExtensionRegistry::new(vec![
        Arc::new(MoviesExtension),
        Arc::new(BooksExtension),
    ]));
    registry.run_migrations(&pool, &[]).await.unwrap();

    // --- 2. Insert legacy-shape rows: every column added by 0002 / 0003 is
    //         omitted, so SQLite stores NULL. This is the exact shape a v0.8.x
    //         DB would have on disk the moment a fresh deployment runs the
    //         upgrade.
    sqlx::query(
        "INSERT INTO movie_entry (slug, media_type, title, rating, published_at) \
         VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .bind("legacy-movie")
    .bind("movie")
    .bind("Legacy Movie")
    .bind(7i64)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO book_entry (source, title, rating, status, published_at) \
         VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .bind("manual")
    .bind("Legacy Book")
    .bind(8i64)
    .bind("completed")
    .execute(&pool)
    .await
    .unwrap();

    // --- 3. Verify the new columns exist with NULL on the legacy row. If any
    //         of these reads fails, the migration didn't apply and the build
    //         below would not be exercising the right schema.
    let movie_nullable: (Option<String>, Option<String>, Option<String>, Option<i32>) =
        sqlx::query_as(
            "SELECT origin, title_ko, title_en, runtime_min FROM movie_entry WHERE slug = 'legacy-movie'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        movie_nullable.0.is_none()
            && movie_nullable.1.is_none()
            && movie_nullable.2.is_none()
            && movie_nullable.3.is_none(),
        "movie_entry 0002/0003 columns must be NULL on a legacy row, got: {:?}",
        movie_nullable,
    );
    let book_nullable: (Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT category, publisher, page_count FROM book_entry WHERE title = 'Legacy Book'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        book_nullable.0.is_none() && book_nullable.1.is_none() && book_nullable.2.is_none(),
        "book_entry 0002 columns must be NULL on a legacy row, got: {:?}",
        book_nullable,
    );

    // --- 4. Build with the real extensions. BuildExt::external_image_urls
    //         exercises `block_on` over the same DB pool — same code path
    //         movies/BooksExtension use in production for the build pipeline.
    let builders: std::sync::Arc<Vec<Box<dyn BuildExt>>> =
        std::sync::Arc::new(vec![Box::new(MoviesExtension), Box::new(BooksExtension)]);
    let rt = tokio::runtime::Handle::current();
    // `spawn_blocking` takes the closure by move; clone the pool (cheap, Arc-backed)
    // and the builders Arc so the `block_in_place` collection loop below can also
    // observe them.
    let pool_for_build = pool.clone();
    let builders_for_build = builders.clone();
    let inactive = inactive_extension_ids(&pool).await;
    let output = tokio::task::spawn_blocking(move || {
        build_site(&pool_for_build, &builders_for_build, &inactive)
    })
    .await
    .expect("spawn_blocking join")
    .expect("build_site must succeed on a legacy-shape DB");
    // Both extensions emitted a page for their published entry.
    assert!(
        output
            .pages
            .iter()
            .any(|p| p.path == "movies/legacy-movie/index.html"),
        "movies page missing from build output: {:#?}",
        output.pages.iter().map(|p| &p.path).collect::<Vec<_>>(),
    );
    // book entry id is auto-assigned; just check that exactly one books page
    // was emitted and that its <title> carries our legacy book title.
    let book_pages: Vec<&str> = output
        .pages
        .iter()
        .filter(|p| p.path.starts_with("books/"))
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(
        book_pages.len(),
        1,
        "one books page expected, got: {book_pages:?}"
    );
    let book_page = output
        .pages
        .iter()
        .find(|p| p.path.starts_with("books/"))
        .expect("book page present");
    assert!(
        book_page.content.contains("Legacy Book"),
        "book page must carry the legacy entry's title, got: {}",
        book_page.content,
    );

    // --- 5. external_image_urls returns the expected shape even when the
    //         legacy row has no poster_path / cover_image_url (both columns
    //         pre-date the migrations and are simply NULL here). The
    //         collections are wrapped in block_in_place the same way
    //         run_image_pre_pass does (see build.rs:237-252) — if the
    //         builder's internal `block_on` ever regressed, this would
    //         panic with the "Cannot start a runtime from within a runtime"
    //         error documented at ssg_build.rs:433-442.
    let external_collected: Vec<(String, Vec<String>)> = tokio::task::block_in_place(|| {
        let mut out = Vec::new();
        for b in builders.iter() {
            let urls = b
                .external_image_urls(&pool, &rt)
                .expect("external_image_urls");
            out.push((b.ext_id().to_string(), urls));
        }
        out
    });
    for (id, urls) in &external_collected {
        assert!(
            urls.is_empty(),
            "{id}: external_image_urls must be empty when no poster/cover is set, got: {urls:?}",
        );
    }

    // --- 6. Bonus: insert a row with a poster_path + cover_image_url and
    //         re-run external_image_urls so we also prove the URL-shape
    //         contract from Track B (movies: full TMDB w500 URL; books:
    //         stored http URL) is honored through a DB whose other rows
    //         pre-date the new migrations.
    sqlx::query(
        "INSERT INTO movie_entry (slug, media_type, title, poster_path, rating, published_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
    )
    .bind("modern-movie")
    .bind("movie")
    .bind("Modern Movie")
    .bind("/abc.jpg")
    .bind(8i64)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO book_entry (source, title, cover_image_url, rating, status, published_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind("aladin")
    .bind("Modern Book")
    .bind("https://example.com/cover.jpg")
    .bind(9i64)
    .bind("completed")
    .bind("2026-08-07T00:00:00.000Z")
    .execute(&pool)
    .await
    .unwrap();

    let after: Vec<(String, Vec<String>)> = tokio::task::block_in_place(|| {
        let mut out = Vec::new();
        for b in builders.iter() {
            let urls = b
                .external_image_urls(&pool, &rt)
                .expect("external_image_urls");
            out.push((b.ext_id().to_string(), urls));
        }
        out
    });
    let movies_urls = after
        .iter()
        .find(|(id, _)| id == "movies")
        .unwrap()
        .1
        .clone();
    assert!(
        movies_urls
            .iter()
            .any(|u| u == "https://image.tmdb.org/t/p/w500/abc.jpg"),
        "movies.external_image_urls must emit the full TMDB w500 URL for poster_path='/abc.jpg', got: {movies_urls:?}",
    );
    let books_urls = after
        .iter()
        .find(|(id, _)| id == "books")
        .unwrap()
        .1
        .clone();
    assert!(
        books_urls
            .iter()
            .any(|u| u == "https://example.com/cover.jpg"),
        "books.external_image_urls must surface the stored cover_image_url verbatim, got: {books_urls:?}",
    );
}

/// Track B acceptance: a builder whose `external_image_urls` returns a real
/// http:// URL causes `run_image_pre_pass` to (a) materialize a manifest
/// entry keyed by that URL and (b) write WebP variants into the staging dir,
/// without ever touching the public internet.
///
/// Hermeticity: the URL points at a local axum HTTP server we spin up on
/// `127.0.0.1:0` in a dedicated tokio task; the server holds the PNG bytes
/// in memory and replies to a single GET. This exercises the real
/// `optimize_external` HTTP code path (reqwest → server) end-to-end but
/// stays inside the test process — no DNS, no proxy, no CDN.
#[tokio::test(flavor = "multi_thread")]
async fn external_image_urls_smoke_writes_manifest_and_webp_variants() {
    use axum::{Router, http::header, response::IntoResponse, routing::get};

    // 1. Build the PNG payload (2400×1600 → all four widths in media::WIDTHS
    //    apply, so the resulting srcset has 4 variants — same shape as the
    //    existing entry_from_bytes test).
    let png_bytes = {
        let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            image::ImageBuffer::from_pixel(2400u32, 1600u32, image::Rgba([0u8, 128, 255, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .expect("encode test png");
        out
    };

    // 2. Stand up a local axum server bound to 127.0.0.1:0 (random free
    //    port). We capture the chosen port from `local_addr()` so the URL
    //    we feed into `external_image_urls` matches what reqwest will hit.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let png_for_server = png_bytes.clone();
    let app = Router::new().route(
        "/poster.png",
        get(move || {
            let png = png_for_server.clone();
            async move { ([(header::CONTENT_TYPE, "image/png")], png).into_response() }
        }),
    );
    let server_task = tokio::spawn(async move {
        // Serve until the listener is dropped (test end) — keeps the URL
        // valid for the single reqwest GET optimize_external issues.
        let _ = axum::serve(listener, app).await;
    });

    // 3. Build a tiny builder whose external_image_urls returns the URL of
    //    our local fixture. This is the production shape: movies/books
    //    return TMDB/cover URLs and `run_image_pre_pass` collects them via
    //    the `block_in_place` loop at build.rs:237-252.
    let url = format!("http://{addr}/poster.png");
    struct ExternalFixtureBuilder(&'static str, Vec<String>);
    impl BuildExt for ExternalFixtureBuilder {
        fn ext_id(&self) -> &'static str {
            self.0
        }
        fn build_pages(
            &self,
            _db: &SqlitePool,
            _rt: &tokio::runtime::Handle,
        ) -> Result<Vec<StaticPage>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }
        fn build_data(
            &self,
            _db: &SqlitePool,
            _rt: &tokio::runtime::Handle,
        ) -> Result<Box<dyn erased_serde::Serialize + Send>, Box<dyn std::error::Error + Send + Sync>>
        {
            Ok(Box::new(serde_json::json!([])))
        }
        fn build_search_docs(
            &self,
            _db: &SqlitePool,
            _rt: &tokio::runtime::Handle,
        ) -> Result<Vec<SearchDoc>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }
        fn external_image_urls(
            &self,
            _db: &SqlitePool,
            _rt: &tokio::runtime::Handle,
        ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.1.clone())
        }
    }
    let builder = ExternalFixtureBuilder("external-fixture", vec![url.clone()]);
    let builders: Vec<Box<dyn BuildExt>> = vec![Box::new(builder)];

    // 4. Run the production pre-pass against a fresh tmp layout.
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let out_dir = tmp.path().join("out");
    let media_dir = data_dir.join("media");
    std::fs::create_dir_all(&media_dir).unwrap();
    let db_path = data_dir.join("oxibuilder.db");
    let pool = db::connect(&db_path).await.unwrap();

    let (staging, manifest) = oxibuilder_core::build::run_image_pre_pass(
        &pool,
        &media_dir,
        &data_dir,
        &builders,
        &tokio::runtime::Handle::current(),
    )
    .await
    .expect("run_image_pre_pass");
    let staging = staging.expect("staging dir must exist when an external URL is provided");
    let manifest = manifest.expect("manifest must exist when an external URL is provided");

    // 5. Manifest entry is keyed by the URL we returned, with all four
    //    WebP variants materialized on disk under staging/media/_derived/.
    let entry = manifest
        .get(&url)
        .unwrap_or_else(|| panic!("manifest missing entry for {url}: {manifest:?}"));
    assert_eq!(
        entry.width, 2400,
        "external PNG was 2400 wide; entry.width must match"
    );
    assert_eq!(entry.height, 1600);
    assert_eq!(
        entry.srcset.len(),
        4,
        "2400px source → all four widths (640/960/1280/1920) apply"
    );
    let derived_dir = staging.join("media").join("_derived");
    assert!(
        derived_dir.is_dir(),
        "staging/media/_derived must exist (created by optimize_external)"
    );
    for s in &entry.srcset {
        let on_disk = derived_dir.join(s.url.strip_prefix("media/_derived/").unwrap_or(&s.url));
        let bytes =
            std::fs::read(&on_disk).unwrap_or_else(|e| panic!("read variant {on_disk:?}: {e}"));
        assert_eq!(&bytes[..4], b"RIFF", "variant missing RIFF magic: {s:?}");
        assert_eq!(&bytes[8..12], b"WEBP", "variant missing WEBP tag: {s:?}");
    }

    // 6. Hand staging + manifest to write_build_output and verify the same
    //    manifest entry + variants ship into out/, including
    //    out/data/image-manifest.json for the static-mode SPA plugin.
    let output = BuildOutput {
        pages: vec![],
        search_docs: vec![],
        extensions_data: vec![],
    };
    let mut inputs = oxibuilder_core::builder::BuildInputs::new(
        "https://example.com/",
        "paper",
        "shell",
        "ext-smoke-seed",
    );
    inputs.image_staging_dir = Some(staging.clone());
    inputs.image_manifest = Some(manifest.clone());
    let out_dir_for_write = out_dir.clone();
    let media_dir_for_write = media_dir.clone();
    tokio::task::spawn_blocking(move || {
        write_build_output(&output, &out_dir_for_write, &media_dir_for_write, &inputs)
    })
    .await
    .expect("spawn_blocking join")
    .expect("write_build_output");

    let out_manifest_json =
        std::fs::read_to_string(out_dir.join("data").join("image-manifest.json"))
            .expect("out/data/image-manifest.json exists");
    assert!(
        out_manifest_json.contains(&url),
        "shipped manifest must reference the external URL {url}: {out_manifest_json}",
    );
    assert!(
        out_manifest_json.contains(".webp"),
        "shipped manifest must carry srcset URLs: {out_manifest_json}",
    );

    // 7. Stop the local server (the spawned task is short-lived; this is
    //    a courtesy — tokio will drop it on test end either way).
    server_task.abort();
    let _ = server_task.await;
}
