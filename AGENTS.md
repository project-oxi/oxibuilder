# AGENTS.md — oxibuilder

> Personal site/blog SSG: Rust + React SPA, single binary, Node-free at runtime.
> CLI-first: every command except `build`/`console` is an authenticated HTTP call
> to a running console. Use `--json` for machine-readable (agent) output.

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
`oxibuilder blog new|list|publish|rm` · `oxibuilder link add|list|rm` · `oxibuilder project ...`

Console-required commands:
`site add|use|list|show|rm` · `mount add|list|rm` · `lobby layout <ext> --mode grid|canvas|list` · `extension enable|disable <name>` · `cache refresh` (GitHub/TMDB/알라딘) · `backup`/`restore` (SQLite snapshot) · `query "SELECT ..."` (read-only SQL) · `schema` (DB schema)

Global flags (all env-overridable): `--endpoint` (`OXIBUILDER_ENDPOINT`), `--site`, `--token`, `--config`, `--json`, `--insecure`.

## Static mounts

Graft an external directory (hand-built HTML or another SSG's output) at a URL prefix:

```bash
oxibuilder mount add --id portfolio --source ../portfolio/dist --path portfolio \
  --title-ko 포트폴리오 --title-en Portfolio --desc "Selected work" [--icon 🖼️] [--new-tab]
```

- Reserved path prefixes (rejected): `assets data media api search s admin lobby theme`.
- `source` is relative to `oxibuilder.toml`'s dir (or absolute); stored verbatim, resolved to absolute only at build time.
- Live example: `a7garden.github.io` mounts `../portfolio/dist` at `/portfolio/`.

## Conventions

- Commit `oxibuilder.toml`; NEVER commit `out/`, `data/*.db`, `web/dist*` (build outputs, gitignored).

## Reference

- CLI dispatch tree: `crates/oxibuilder-cli/src/main.rs` (`Command` enum).
- Mount endpoints: `crates/oxibuilder-console/src/router.rs` (`/mounts` handlers).
