# Update dependencies

run cargo commands inside `kuterm-dev` container (see AGENTS.md).

## crates.io deps

- `cargo update`: bump within semver ranges (Cargo.lock only).
- `cargo upgrade --incompatible`: bump past ranges, edits Cargo.toml (needs `cargo-edit`).
- `cargo outdated`: list outdated deps (needs `cargo-outdated`, ignores git deps).

## git deps (gpui, gpui_platform, alacritty_terminal)

pinned by `rev`, so `cargo update` won't move them.

1. `git ls-remote https://github.com/zed-industries/zed HEAD` -> new zed rev.
2. set it on all 3 gpui entries (`gpui`, `gpui_platform`, dev-dep `gpui`).
3. `curl -s https://raw.githubusercontent.com/zed-industries/zed/<REV>/Cargo.toml | grep alacritty` -> set that rev on `alacritty_terminal` (must match, shared types).

## verify

```bash
sh .zed/podman-build-test.sh "$PWD"
cargo fmt --check && cargo clippy --all-targets -- -D warnings
```
