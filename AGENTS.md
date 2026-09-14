# AGENTS.md — oxibuilder

> Personal site/blog SSG: Rust + React SPA, single binary, Node-free at runtime.
> CLI-first: every command except `build`/`console` is an authenticated HTTP call
> to a running console. Use `--json` for machine-readable (agent) output.


## Mission — 자기발전형 블로그

a7garden.github.io는 이 도구(oxibuilder)로 만드는 블로그이고, 이 블로그를
개선하는 유일한 방법은 oxibuilder를 개선하는 것이다. 즉 이 저장소에서 작업하는
에이전트는 **블로그의 자기발전을 담당한다**: 블로그 품질 요구(디자인 완성도,
데이터 이관, 배포 안정성)가 곧 oxibuilder의 백로그다.

- 기준선: 2026-08-11 Astro 시대 사이트 (`a7garden.github.io` 커밋 `ae46813`) —
  영화/도서 포스터 그리드, SUIT/SUITE 폰트, 다크 테마. oxibuilder 셸·확장
  페이지가 이 완성도에 도달하면 레거시 마운트를 네이티브 확장으로 전환한다.
- 사이트 디렉터리: `~/Documents/Workspace/Projects/a7garden.github.io`
  (DB `data/oxibuilder.db`, 배포는 `oxibuilder deploy` — main 전체 교체 push).
- 레거시 마운트(portfolio/movies/books/posters/book-covers + `_astro`)는
  네이티브 전환 완료 전까지 데이터 보존 상태로 유지한다.

## Project Stack

- Rust workspace (`crates/oxibuilder-cli` etc.). Console = HTTP server; static output in `out/`.
- Content (posts/links/projects) lives in a per-site SQLite DB, managed via CLI.
- `oxibuilder.toml` = single source of truth for settings AND mounts; console APIs patch it on disk and live-reload the in-memory snapshot.
- Sites registry: `~/Library/Application Support/dev.oxibuilder.oxibuilder/sites.toml`.

## Commands

```bash
cargo install --path crates/oxibuilder-cli --force   # → ~/.cargo/bin/oxibuilder
oxibuilder init [--wizard]                           # scaffold oxibuilder.toml
oxibuilder console [--port N] [--preview]            # boot console (most commands need it)
oxibuilder status                                    # drafts / recent / server status
oxibuilder build                                     # static build → out/
oxibuilder deploy                                    # GitHub Pages ([deploy.github_pages])
```

Content commands (write the per-site SQLite DB directly — no console required):
`oxibuilder blog new|edit|list|publish|rm` (`new --translation-of <id>` links a translation; `--category`, `--series`/`--order`/`--clear-series`) · `oxibuilder blog series new|list|show` · `oxibuilder link add|list|rm` · `oxibuilder project ...`

Console-required commands:
`site add|use|list|show|rm` · `mount add|list|rm` · `lobby layout <ext> --mode grid|canvas|list` · `extension enable|disable <name>` · `cache refresh` (GitHub/TMDB/알라딘) · `backup`/`restore` (SQLite snapshot) · `query "SELECT ..."` (read-only SQL) · `schema` (DB schema)

Global flags (all env-overridable): `--endpoint` (`OXIBUILDER_ENDPOINT`), `--site`, `--token`, `--config`, `--json`, `--insecure`.

## Static mounts

Graft an external directory (hand-built HTML or another SSG's output) at a URL prefix:

```bash
oxibuilder mount add --id portfolio --source ../portfolio/dist --path portfolio \
  --title-ko 포트폴리오 --title-en Portfolio --desc "Selected work" [--icon 🖼️] [--new-tab] [--raw] [--hidden]
```

- Reserved path prefixes (rejected): `assets data media api search s admin lobby theme`.
- `raw = true` grafts the directory as-is, skipping static-output detection (asset bundles with no `index.html`); `hidden = true` keeps the mount out of the lobby manifest.
- `source` is relative to `oxibuilder.toml`'s dir (or absolute); stored verbatim, resolved to absolute only at build time.
- **File mounts**: if `source` is a single FILE, it is copied to `out/<path>` verbatim (e.g. `--path favicon.svg --source legacy-root/favicon.svg --raw --hidden`) — for root-level files a legacy build expects. Mount paths may not end in `index.html`.
- **Legacy shared assets**: a grafted legacy SSG build usually references shared assets at its SOURCE site root (`/_astro/*.css`). A directory graft does NOT carry those — every mounted page 404s its stylesheet. Fix: keep the directory next to the site and add a raw hidden mount at the matching path (`--path _astro`). The build lints this: unresolved root-absolute refs in mounted pages print a `warning:` naming the missing segment.
- Lobby path collisions (extension vs mount at the same path): the MOUNT wins the card, mirroring graft order in `write_build_output`. Since v0.10 the build pipeline also SKIPS extensions disabled in `extension_state` (previously they still emitted pages, mixing with mounts at the same path).
- Live example: `a7garden.github.io` mounts `portfolio`, plus raw hidden `_astro` (shared CSS) and `favicon.svg` (file mount) from its Astro era.

## Conventions

- Commit `oxibuilder.toml`; NEVER commit `out/`, `data/*.db`, `web/dist*` (build outputs, gitignored).

## Reference

- CLI dispatch tree: `crates/oxibuilder-cli/src/main.rs` (`Command` enum).
- Mount endpoints: `crates/oxibuilder-console/src/router.rs` (`/mounts` handlers).
