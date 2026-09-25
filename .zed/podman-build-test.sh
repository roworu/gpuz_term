#!/bin/sh
podman build -q -t gpuz_term-dev - < Containerfile && \
podman run --rm -v "${1}":/src:Z \
  -v gpuz_term-cargo-registry:/usr/local/cargo/registry \
  -v gpuz_term-cargo-git:/usr/local/cargo/git \
  gpuz_term-dev sh -c 'cargo build && cargo test'
