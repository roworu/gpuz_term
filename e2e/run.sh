#!/bin/sh
# e2e tests: check the real binary by pixels on xvfb or headless sway, all inside podman.
# usage: e2e/run.sh [--backend x11|wayland] [pytest args], e.g. -k cursor or -x
# KUTERM_BIN=path/to/kuterm tests that binary, otherwise a debug build is made first.
# the report with screenshots of every tested feature is written to e2e/artifacts/report-<backend>.html.
# exits non-zero when any test fails
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
set -e
backend="${E2E_BACKEND:-x11}"
if [ "$1" = "--backend" ]; then
  backend="$2"
  shift 2
fi
case "$backend" in
  x11|wayland) ;;
  *) echo "unknown backend $backend, use x11 or wayland" >&2; exit 2 ;;
esac
podman build -q -t kuterm-e2e -f "$ROOT/e2e/Containerfile" "$ROOT/e2e" >/dev/null
if [ -n "$KUTERM_BIN" ]; then
  mkdir -p "$ROOT/target/e2e"
  cp "$KUTERM_BIN" "$ROOT/target/e2e/kuterm"
  bin=/src/target/e2e/kuterm
else
  podman build -q -t kuterm-dev - < "$ROOT/Containerfile" >/dev/null
  podman run --rm -v "$ROOT":/src:Z \
    -v kuterm-cargo-registry:/usr/local/cargo/registry \
    -v kuterm-cargo-git:/usr/local/cargo/git \
    kuterm-dev cargo build --quiet
  bin=/src/target/podman/debug/kuterm
fi
# the other backend's report stays
rm -rf "$ROOT/e2e/artifacts/report-$backend.html" "$ROOT/e2e/artifacts/shots-$backend"
set +e
# sway and kuterm share 4k frame buffers through /dev/shm, the 64 MB default is too small
podman run --rm --init --memory 4g --shm-size 1g -v "$ROOT":/src:Z \
  -e KUTERM_BIN="$bin" -e E2E_BACKEND="$backend" -e PYTHONDONTWRITEBYTECODE=1 \
  kuterm-e2e python3 -m pytest -p no:cacheprovider -v -rfEs --tb=short e2e "$@"
status=$?
echo "report: e2e/artifacts/report-$backend.html"
if [ $status -ne 0 ]; then
  echo "=== E2E FAILED (exit $status) ==="
fi
exit $status
