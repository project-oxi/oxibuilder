# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Blog series + categories.** `blog_series` table (migration 0002) with per-post category and series ordering; CLI `blog series new|list|show` plus `--category` / `--series` / `--order` / `--clear-series` on `blog new`/`blog edit`; console `/series` API; the static build emits series pages (only when published posts exist) and `build_data` returns `{posts, series}`; the SPA gains a series page, category filter, and series navigation; the admin BlogTab gains a category input.
- **Post translations.** `blog new --translation-of <id>` links a post to its original's translation group (the group id is promoted from the original); list and post pages surface language switch links (ko/en).
- **Raw + hidden mounts.** `[[mounts]]` gained `raw = true` (graft the directory as-is, skipping static-output detection — for asset bundles with no `index.html`) and `hidden = true` (exclude the mount from the lobby manifest).
- **Single-file mounts.** A mount whose `source` is a file is copied to `out/<path>` verbatim — graft root-level files a legacy build expects (e.g. `--path favicon.svg --source legacy-root/favicon.svg --raw --hidden`). Mount paths may not end in `index.html` (rejected at validation; would clobber the page at that path).
- **Mount asset lint.** After grafting, the build scans mounted HTML for root-absolute references whose first path segment nothing in the build provides — the classic legacy-SSG graft losing its site-root assets (e.g. `/_astro/BaseLayout.css` 404ing on every mounted page) — and prints a warning naming the mount, the missing URLs, and the file-mount fix.

### Fixed
- **Disabled extensions no longer emit static output.** The build pipeline ignored `extension_state`; disabled extensions still wrote pages, data, and search docs, silently mixing with static mounts grafted over the same path (legacy `/movies` list page + native `movies/{slug}` detail shells). `build_site` / `build_site_with_progress` now gate every builder by the same `extension_state` the live route dispatcher applies (batch `manifest::inactive_extension_ids`, resolved by callers on an async context), and the image pre-pass skips disabled extensions' external URLs.
- **Lobby card duplication on path collisions.** When an active extension and a visible static mount share a path, the manifest now emits only the mount card — mounts graft after extension pages in `write_build_output`, so the mount owns the serving path.
- **GitHub Pages silently dropped `_`-prefixed output.** Without a `.nojekyll` marker, Pages runs Jekyll, which discards every `_`-prefixed directory — a legacy `/_astro` asset mount 404'd in production while the local build was fine. `write_build_output` now emits an empty `.nojekyll` at the output root (harmless on non-Pages hosts).
- **Movies poster refs.** Multi-segment absolute poster paths (local mounted refs like `/posters/x.jpg`) are no longer fetched as TMDB URLs at build time or prefixed with `image.tmdb.org` in the SPA; only single-segment raw TMDB paths get the `w500`/`w{width}` prefix.

## [0.10.0] - 2026-08-08

### Added
- **External static mounts.** `[[mounts]]` config grafts an external static directory at a URL prefix, copied into `out/{path}/` at build time and shown as a lobby link card. No-match mounts are dropped instead of copying the project root into `out/`.
- **Mount source auto-detection.** `detect_static_output` and `resolve_mount_sources` scan candidate build-output directories under a mount source and auto-select the static output (candidates scanned before the root override), so users point at a folder and the build artifact is found automatically.
- **CLI + console mount management.** Static mounts are managed via `oxibuilder` CLI commands and `POST /api/console/mounts`; `GET /api/console/mounts` surfaces the resolved mount source.
- **Editorial layout variant.** A new lobby/page layout with full feature parity (stats, filters, detail views, fonts).
- **Movies/books blog-test parity.** Stats dimensions (movie origin/country + nations, book category/publisher/page_count), book category chips, and build-time external-image optimization (TMDB posters, book covers → responsive WebP via `media::optimize_external` + `BuildExt::external_image_urls`).
- **Bilingual movies.** Movies support bilingual titles, genres, and cast with a filterable page; `media_type`/`rating` default correctly and the site slug is resolved.
- **Profile CLI + theme snapshot.** `oxibuilder` profile command, theme snapshot, and site footer.
- **Oxi-brand console styling.** Admin UI restyled with the oxi-brand design system.

### Fixed
- **Mount config.** `detect_static_output` scans candidates before applying the root override, preventing the project root from being copied into `out/` on a no-match mount (covered by a new regression test).
- **Web theme trigger.** Light/dark trigger migrated `[data-theme]` → `.dark` class.
- **Movies defaults.** `media_type`/`rating` now default correctly; the site slug is resolved for movie routes.
- **Release pipeline.** `release.yml` publishes `oxibuilder-deploy` before `oxibuilder-console` (dependency order).
- **Stats tests.** Corrected import paths and `bun:test` `ts-expect-error`.
- **CLI.** `ProfileCommand::Set` boxed to clear the `large_enum_variant` clippy lint; rustfmt'd.

### Security
- **wasmtime 33.0.2 → 36.0.13.** Resolves 17 outstanding advisories against the EOL 33.x line, including the two criticals tracked since v0.8.0 (RUSTSEC-2026-0095 Winch sandbox escape — not compiled in; RUSTSEC-2026-0096 aarch64 Cranelift heap miscompile — conditional/mitigated). The bump cascades through `oxibuilder-wasm`'s API surface.

## [0.9.0] - 2026-08-05

### Added
- **Inline media authoring.** Images and GIFs can be embedded inside markdown content bodies (blog posts, project descriptions, profile bios, book/movie/scraps reviews, novel chapters). The `MarkdownEditor` gained an image toolbar with a `MediaPicker` (browse/upload/delete/pick), plus drag-and-drop and paste-to-upload that splice `![alt](media/<ext>/<uuid>)` at the cursor. Markdown rendering (`markdown-it` for the public SPA, `marked` for the admin preview) now resolves `media/...` references through a shared `AssetResolverContext`, so inline images render correctly in the live admin, draft preview, built preview, and on nested deployed routes.
- **Media library API.** `GET /api/console/s/{slug}/media` enumerates uploaded media (filterable by extension, newest-first); `DELETE /api/console/s/{slug}/media/{ext}/{file}` removes one. Both reuse the existing path-containment checks.
- **Rust-native markdown rendering + image optimization.** `oxibuilder-core` gained a `pulldown-cmark`-based `markdown::render` that rewrites `media/...` references into optimized `<img>` tags from an `ImageManifest`; `media::optimize` generates responsive WebP variants (srcset + intrinsic dimensions) with an on-disk cache. Derived images are staged outside `out/` and copied in after the wipe, so the cache and derived WebP survive rebuilds.
- **Prerendered blog pages.** The SSG build renders each post's markdown body into the SPA's `#root` shell at build time, emitting single-slash (apex-correct) image URLs and a reactive SPA manifest; the public SPA consumes the image manifest via a markdown-it plugin so prerendered and client-rendered pages agree.
- **Branding.** The oxibuilder icon is applied across the public site and console; the project was renamed `oxipage` → `oxibuilder`.

### Fixed
- **Admin SPA rendered blank on every page** (React error #310). `SiteSelector` called `useQuery` after an early `return null`, so the hook count rose once the sites list loaded. All hooks now run unconditionally; the stats fetch is gated via `enabled`.
- **`adminAssetResolver` produced a doubled `media/media/` path**, which 404'd cover previews (and inline-image previews) against the single-`media` serve route.
- **Single-slash image URLs (apex-correct)** in prerendered pages; the SPA manifest is resolved reactively under `<base>`.
- **Console media library `unnecessary_sort_by` clippy lint** — newest-first sort rewritten with `sort_by_key(Reverse)`.

### Security
- 18 outstanding advisories (soft gate): 17× `wasmtime 33.0.2` (up from 17 — RUSTSEC-2026-0091 added) and 1× `rsa 0.9.10` Marvin Attack (timing sidechannel). wasmtime 33.x is EOL; the two critical advisories are unchanged from v0.8.0 (RUSTSEC-2026-0095 Winch not compiled in, RUSTSEC-2026-0096 aarch64 Cranelift conditional). Bump to a supported major is tracked separately.

## [0.8.0] - 2026-07-31

### Added
- **Theme system.** A single `ThemeDefinition` catalog (`paper`, `midnight`, `sepia`, `forest`, `neon`, `canvas`) lives in `oxibuilder-core`; the per-site `/theme` API returns the full definition; the console's Appearance section exposes a three-state toggle (system / light / dark) backed by a shared `theme-boot.js` that replaces the duplicated inline FOUC scripts.
- **Materialization.** The SSG build now emits relative asset tags plus a deployment `<base href>`, writes a `BuildManifest` (`deployment_base`, `theme_id`, `asset_revision`, `build_id`) via `derive_deployment_base` from `site.base_url`, and computes a deterministic SHA-256 asset revision over `web/dist` (baked into embedded crates as `OXIBUILDER_SPA_REVISION`).
- **Console media upload.** `POST /api/console/.../media` accepts multipart uploads validated by magic bytes; uploaded media is served live from the site's media dir.
- **Console preview.** Preview is prefix-aware, rewrites `<base>` for project-page deploys, and returns `424 Build Required` when the site hasn't been built yet.
- **Console deploy surface.** GitHub Pages settings are now mutable (`PATCH`), deploy history is persisted per site, preflight + reconnect APIs were added, and build/deploy operations are serialized per site with an atomic `config_write_lock` (`MutableSiteSettings`).
- **Authoring improvements.** ProfileTab, atomic reorders with shared validators, and blog server-side validation.
- **Admin editor UX.** `EditorPreviewDrawer` (2-pane desktop / mobile tabs), `DraftPreviewPane` (distinct from Preview Site), `ImageField` + `AssetResolver` + `uploadImage` + Preview Site button, `TagInput` chip editor, field-level validation helpers surfaced in `jsonOrThrow`, and an `ErrorBoundary` with stale-chunk recovery.
- **Profile optimistic concurrency.** `PUT` carries `expected_updated_at`; conflicts are rejected instead of silently overwriting.
- **Repository-scoped GitHub Pages deployment.** `oxibuilder-core` gained validated GitHub Pages target config; deploy is scoped to the target repository.

### Changed
- **GitHub Pages deployment is repository-scoped** (`DeployTarget` instead of repo-inferred); the legacy top-level console build/deploy routes were removed in favor of the site-scoped ones.
- `SiteContext` resolves absolute paths for `project_dir` / `data_dir` / `out_dir` / `media_dir`.
- Shared presentation components extracted (`*Card`, `ProjectView`, `ProfileView`, `BlogPostView`, `BlogPostCard`); sidebar uses semantic theme tokens; console appearance is decoupled from the public site theme.
- Docs/plan references migrated from `a7garden/oxibuilder` to `project-oxi/oxibuilder`.

### Fixed
- `/preview/*` non-API paths return 404 instead of the SPA fallback.
- Movies search surfaces a TMDB-disabled hint; books normalize legacy `read` / `dnf` status on read.
- `oxibuilder deploy` honors the registered site; dead `/vite.svg` favicon replaced with an inline data URI.
- Workspace `cargo fmt` + clippy 1.96 lint cleanup (`unnecessary_map_or`, `manual_ok_err`, `needless_borrow`, `io_other_error`, `result_unit_err`, `type_complexity`, `collapsible_if`, `useless_format`, unused imports).
- CI: `ci.yml` / `release.yml` now run `bun run build:static` — `oxibuilder-core`'s build.rs requires `web/dist-static` for the materialized static SPA; two tests were decoupled from stale SPA chunk naming.

### Security
- Unchanged from v0.7.0: 17 outstanding `wasmtime 33.0.2` advisories (RUSTSEC-2026-0095 not applicable, RUSTSEC-2026-0096 conditional/mitigated). wasmtime 33.x is EOL; a bump to 36/42/43 cascades through `oxibuilder-wasm`'s API surface and is tracked separately.

## [0.7.0] - 2026-07-30

### Added
- **Site-scoped console.** The console server now operates per-site rather than per-app. The router is mounted under `/api/console`; every extension handler resolves through `SiteScopedDb` middleware that picks the right `console.db` from the request's site slug.
- **Site directory wizard + `/s/{slug}/` redirect.** First-run setup walks the user through creating a site (slug, name, base URL, languages, enabled extensions). The `/s/{slug}/` URL scheme routes the SPA to the right site.
- **Site-scoped build/deploy/preview endpoints.** `POST /api/console/sites/{slug}/build` runs `oxibuilder build` against the site's working directory; `/deploy` and `/preview` follow the same site-scoped shape.
- **`create-site` CLI handler.** `oxibuilder init console` (or the new site-creation flow) creates the site directory, initializes `console.db`, registers the site, and surfaces the new slug in the site picker.
- **Console-only SPA in `web/src/admin/`.** The old `admin-web/` Vite app was folded into `web/src/admin/` (shared Tailwind v4 + OKLCH tokens). One `web/dist` build now ships both lobby and console surfaces.
- **`SitesFile` → `path-only` schema.** Sites are tracked as paths in `~/.config/oxibuilder/sites.toml`, no longer as opaque records.
- **`console.db` with `setup_state` table.** Per-site setup state lives next to the site, not in a global DB.

### Changed
- **BREAKING: `Extension::routes()` now returns `Router` (no state).** Extension handlers receive `Extension<SiteScopedDb>` instead of `State<AppState>`. First-party extensions were updated in the same commit; no third-party consumers exist (the ecosystem is in-tree only).
- **`oxibuilder-server` removed.** The old `:8788` admin module, the `admin-web/` app, and the standalone `admin` CLI subcommand were all deleted; the console absorbs their surface.

### Fixed
- Console router nested under `/api/console` so the SPA's static_handler no longer shadow-serves the API.
- E2E CLI tests use `--path` for the site root (post schema migration).
- `oxibuilder.toml` name + extensions corrupted from a merge smoke test — restored the original config.

### Style
- Workspace-wide `cargo fmt` pass (whitespace only).

## [0.6.0] - 2026-07-29

### Added
- **Per-extension setup sub-wizards.** Extensions that own external API keys (`movies`/TMDB, `books`/Aladin + Google Books, `activity`/GitHub) now declare their own multi-step `setup_wizard` instead of relying on a shared keys step.
  - `SetupFieldKind::Secret` for API keys (rendered as password, never prefilled).
  - Declarative `visible_when` rules (`VisibilityRule`) evaluated client-side via `visibility.ts` (`evalRule` / `mergeOutcome` / `resolvePrefill`).
  - **Action steps** run live checks after key entry: `movies_test` / `books_test` call the real external API and report `connection_ok`; `activity_sync` immediately syncs public GitHub activity (`repo::upsert`) and reports `synced`. Each action step shows conditionally on its key-step result.
- `ExtensionSubWizard` (admin-web): owns one extension's sub-wizard with internal step nav, outcome accumulation, and `visible_when` filtering. `GenericStep` now renders action steps (empty field sets) and `Secret`/password fields.
- `SetupSaveHandler::save` returns `StepOutcome` (values used to evaluate downstream `visible_when` / prefill); `setup_extension_step_handler` returns `DataEnvelope<StepOutcome>`.
- `persist_extension_config` is now `pub`, so each extension's key-step save handler persists its own config directly.

### Changed
- **`external_api_keys` mechanism removed.** The centralized `Extension::external_api_keys()` / `save_external_key()` trait methods, `ExternalApiKey` / `ExternalKeyScope` types, the `/setup/external-keys` route + handler, `StatusResult.external_api_keys`, the matching OpenAPI entries, and the front-end `ExternalKeysStep.tsx` + `submitExternalKeys` were all deleted. API keys now live in each extension's own `setup_wizard` key-step and persist via `persist_extension_config`. First-party-only extension ecosystem — no third-party consumers affected.
- **Setup wizard API renamed.** `Extension::setup_wizard_step(Option<SetupStep>)` → `setup_wizard(Option<ExtensionWizard { steps }>)`; `StatusResult.extension_steps` → `extension_wizards`; route `/setup/extension-step/{id}` → `/setup/extension-step/{ext_id}/{step_id}` (namespaced per extension).

### Fixed
- Clippy `-D warnings` lints in extension setup save handlers: `oxibuilder-ext-activity` (`collapsible_if` → let-chain), `oxibuilder-ext-books` / `oxibuilder-ext-movies` (redundant `matches!(.., Ok(_))` → `.is_ok()`).

## [0.5.0] - 2026-07-29

### Added
- **oxibuilder build** (SSG): full static-site build pipeline.
  - SSG regression test (`crates/oxibuilder-core/tests/ssg_build.rs`) asserts `out/` layout and asset references.
  - `BuildExt` trait now takes a `tokio::runtime::Handle` so `build_site` runs on rayon workers without panic.
  - `build_writer` sources the SPA from the embedded binary (no CWD `web/dist` dependency), writes `out/index.html`, `out/404.html`, and `out/assets/*` at root, and injects the real hashed script/css into content shells.
  - Extension static-data contract: `fetchBlogPost` loads `blog.json` and `.find(slug)`; `searchAll` client-filters the search index; collection cached at module scope.
  - `oxibuilder build build` flattened to `oxibuilder build`.

### Fixed
- **cli** (`oxibuilder-cli`): member-crate `[profile.release]` with `lto = true`, `codegen-units = 1`, `strip = "none"` so `cargo install oxibuilder` succeeds on macOS 27. The workspace root profile does not ship in the published `.crate` tarball; this restores the workaround for rust-lang/rust#157750 (mis-aligned LINKEDIT string pool in stripped proc-macro dylibs).

### Changed
- **README**: status section no longer says "v2 SSG in design"; install + getting started now reference `oxibuilder-console` (the binary that absorbed `oxibuilder-server` in doc/12); deploy claim now explicitly calls out the bailed Cloudflare Pages / Netlify paths.
- **extension-sdk docs**: replaced "AdminAuth" write-route rule with the loopback-only management-server model; `oxibuilder-server` crate references → `oxibuilder-console`.
- **oxibuilder-cli/SKILL.md**: replaced the removed PAT/auth flow with the loopback-only model.
- **oxibuilder-core/Cargo.toml**: added explicit `include = [...]` so `cargo publish` bundles `embedded-spa/`, `_registry.json`, and `_wasm-demo.wasm` (these are gitignored build artifacts; without `include`, `cargo install oxibuilder` would ship a placeholder SPA and a broken `oxibuilder build`).

### Security
- 17 outstanding `wasmtime 33.0.2` advisories remain, with two CRITICAL:
  - RUSTSEC-2026-0095 (Winch sandbox escape) — **does not apply**: workspace pin uses `features = ["cranelift", "runtime", "parallel-compilation", "cache"]`; the `winch` feature is not enabled, so the vulnerable code path is not compiled in.
  - RUSTSEC-2026-0096 (aarch64 Cranelift heap miscompile) — **conditional**: only affects 64-bit WebAssembly linear memories on aarch64; mitigated by default Spectre mitigations. Real on aarch64 deployments of the published CLI binary; out of scope for this release.
- wasmtime 33.x is no longer receiving security patches. Bumping to a supported major (36 / 42 / 43) is tracked separately; the bump cascades through the `oxibuilder-wasm` API surface (`Config, Engine, Instance, Linker, Module, Store, Caller, Val`) and needs its own release.

## [0.4.0] - 2026-07-29

### Changed
- **admin-web**: adopt Tailwind v4 + OKLCH design token pipeline shared with the public web app. Replaced hardcoded inline styles with utility classes across AdminShell, SiteSwitcher, content / dashboard / settings / extensions / themes pages; added ThemeToggle; extracted input/textarea primitives.

### Style
- Workspace-wide `cargo fmt` pass (whitespace only).

### Notes
- Effective crates.io floor is `0.3.0` (`oxibuilder-wasm@0.3.0` already published under tag v0.3.0); 0.4.0 advances past that burn.
- Continuation of the v0.3.0 line; the v0.3.0 Git tag was applied to a partial-publish state (4 crates were never released: `oxibuilder-ext-scraps`, `oxibuilder-ext-projects`, `oxibuilder-console`, `oxibuilder`). Those crates are not in 0.4.0 — they remain unpublished at 0.2.0 / absent from the registry; future cleanup is a separate concern.


[Unreleased]: https://github.com/a7garden/oxibuilder/compare/v0.10.0...HEAD
[0.10.0]: https://github.com/a7garden/oxibuilder/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/a7garden/oxibuilder/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/a7garden/oxibuilder/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/a7garden/oxibuilder/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/a7garden/oxibuilder/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/a7garden/oxibuilder/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/a7garden/oxibuilder/compare/v0.3.0...v0.4.0
