#!/bin/sh
# e2e tests: check the real binary by pixels on xvfb, all inside podman.
# usage: e2e/run.sh [pytest args], e.g. -k cursor or -x
# KUTERM_BIN=path/to/kuterm tests that binary, otherwise a debug build is made first.
# the report with screenshots of every tested feature is written to e2e/artifacts/report.html.
# exits non-zero when any test fails
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
set -e
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
rm -rf "$ROOT/e2e/artifacts"
set +e
podman run --rm --init --memory 4g -v "$ROOT":/src:Z \
  -e KUTERM_BIN="$bin" -e PYTHONDONTWRITEBYTECODE=1 \
  kuterm-e2e python3 -m pytest -p no:cacheprovider -v -rfEs --tb=short e2e "$@"
status=$?
echo "report: e2e/artifacts/report.html"
if [ $status -ne 0 ]; then
  echo "=== E2E FAILED (exit $status) ==="
fi
exit $status
