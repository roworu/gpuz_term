# Release

pushing a `release/v*` branch runs `.github/workflows/release.yml`. it builds linux, macos and windows binaries, then publishes a github release with checksums and generated notes. the release creates the `v*` tag itself.

1. create the branch from up to date `main`:
   ```bash
   VERSION="v0.0.2"
   git switch main && git pull
   git switch -c "release/$VERSION"
   ```
2. bump `version` in `Cargo.toml` and update `Cargo.lock` (no build, so it works without native deps):
   ```bash
   sed -i "0,/^version = .*/s//version = \"${VERSION#v}\"/" Cargo.toml
   cargo update --workspace
   ```
3. commit and push, which starts the release build:
   ```bash
   git commit -am "release $VERSION"
   git push -u origin "release/$VERSION"
   ```
4. watch the run, then check the release:
   ```bash
   gh run watch
   gh release view "$VERSION"
   ```

every push to the branch rebuilds, but publish fails if the release already exists. to redo a release, delete it and its tag first:
`gh release delete "$VERSION" --cleanup-tag`
