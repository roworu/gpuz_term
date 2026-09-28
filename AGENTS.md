# AGENTS.md

that file provides guidance to AI coding agents on how to work with code in this repo.

gpuz_term is gpu accelerated desktop terminal emulator written in rust: gpui for ui, alacritty for terminal emulation.

## Build and test

host usually lacks native deps (fontconfig, some wayland libs, xkbcommon... etc.), so build inside podman/docker using `Containerfile`:

```bash
# cargo build, clippy, test in container
sh .zed/podman-build-test.sh "$PWD"
# run container-built binary on host
./target/podman/debug/gpuz_term
```

to run single test:

```bash
podman run --rm -v "$PWD":/src:Z \
  -v gpuz_term-cargo-registry:/usr/local/cargo/registry \
  -v gpuz_term-cargo-git:/usr/local/cargo/git \
  gpuz_term-dev cargo test <test_name>
```

CI also runs `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`. container output goes to `target/podman/`. gpui and alacritty are pinned to same git revision in `Cargo.toml`; bump them together.

## Architecture

- `main.rs` `init(cx)` holds all startup setup (fonts, settings, theme, keybindings); keep it separate from `main`.

- `src/terminal/`: terminal model, no ui. `TerminalBuilder::new` spawns shell in a pty; `subscribe` moves it into a gpui `Entity<Terminal>` and pumps alacritty events (`events.rs`). `Terminal::sync` snapshots grid into `last_content` (`content.rs`), which is all renderer reads. `keys.rs` maps keystrokes to escape sequences.

- `src/ui/`: `Workspace` (root view, tabs) -> `TerminalView` (focus, keys, scroll, paste) -> `TerminalElement`, a custom gpui `Element` that sizes terminal in `prepaint`, builds background rects and batched text runs (`grid.rs`), and paints cursor (`cursor.rs`).

- `src/settings/` and `src/theme.rs`: gpui globals read with `Settings::get(cx)` / `Theme::get(cx)`.
  - Defaults live only in `assets/*.jsonc` (embedded as bin with `include_str!`); `Default` impls parse them, so NEVER hardcode defaults in rust as a code.
  - User files in `$XDG_CONFIG_HOME/gpuz_term/` are parsed and deep merged over the bundled json (`settings::merge`); enums and arrays are replaced whole. Missing files are created from defaults, broken ones fall back to defaults with an error on stderr.
  - New setting: add the struct field, add it with a comment to `assets/default_settings.jsonc`. Numeric limits are enforced in `Settings::parse`.
  - New action: add it to `binding()` in `keybindings.rs` and to `assets/default_keybindings.jsonc` with a comment.

## Conventions

Comments explain `why`, not `what` they do. Tests go in a `mod tests` at the bottom of the file they test; terminal tests spawn a real shell.

Every change must pass the full test suite (`sh .zed/podman-build-test.sh "$PWD"`), and new functionality must come with new tests.

Always ask the user to review changes before committing or opening a PR. When acting autonomously (no user in the loop), end the PR description with a line like `Changes prepared by <model>, running <harness>.`
