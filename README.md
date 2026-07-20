# git-workspaces

A manifest-driven multi-repo git plugin. `git workspaces <cmd>` manages a fleet
of independent git repos declared in a `workspaces.toml` at the root of a
parent folder — the pattern you want when several sibling repos live under
one folder and get worked on together.

Install via Homebrew (recommended):

```sh
brew tap nicolaei/tools
brew install git-workspaces
```

## Local install (development)

`git` finds subcommands by looking for an executable named `git-<cmd>`
anywhere on `$PATH`. To make `git workspaces` resolve to this crate during
local development:

1. Build a release binary:

   ```sh
   cargo build --release
   ```

2. Put the binary on `$PATH` as `git-workspaces`. Either symlink it into a
   directory already on your `$PATH` (e.g. `~/.local/bin`, `/usr/local/bin`):

   ```sh
   ln -sf "$(pwd)/target/release/git-workspaces" ~/.local/bin/git-workspaces
   ```

   or add `target/release/` itself to `$PATH`:

   ```sh
   export PATH="$(pwd)/target/release:$PATH"
   ```

3. Verify it resolves:

   ```sh
   git workspaces --version
   ```

   This should print the crate's version — proof that git's own subcommand
   dispatch found the binary, exactly as it would for a real user install.

## Releasing

Releases are built and published entirely by CI ([`dist`][dist], configured in
`dist-workspace.toml`). Cutting a release means bumping the version and pushing
a matching tag — everything else is automatic.

[dist]: https://github.com/axodotdev/cargo-dist

1. Bump `version` in `Cargo.toml` (and run `cargo build` once so `Cargo.lock`
   picks it up). The tag you push below must match this exactly — `dist`
   selects packages to release by matching the tag's version against
   `Cargo.toml`, so a mismatch means nothing gets built or published.

   ```sh
   cargo build --release   # regenerates Cargo.lock for the new version
   ```

2. Commit the version bump:

   ```sh
   git commit -am "chore(release): vX.Y.Z"
   git push
   ```

3. Tag and push the tag — this is what actually triggers the release:

   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. Watch it run:

   ```sh
   gh run list --repo nicolaei/git-workspaces --limit 1
   ```

   The workflow (`.github/workflows/release.yml`) builds macOS binaries for
   both `aarch64-apple-darwin` and `x86_64-apple-darwin`, creates the GitHub
   Release with the built artifacts and a shell installer, then pushes an
   updated formula to the [`nicolaei/homebrew-tools`][tap] tap so
   `brew install git-workspaces` picks up the new version.

   [tap]: https://github.com/nicolaei/homebrew-tools

5. Verify the install path actually works end to end:

   ```sh
   brew update && brew upgrade git-workspaces
   git workspaces --version   # should print the new version
   ```

### Caveats

- **Homebrew only ever keeps the latest version.** There's no way to install
  an older release through the tap once a newer one exists. Don't tag and
  push an old version after a newer one has already gone out — it will
  overwrite the formula backwards and confuse anyone who updates.
- **Prereleases aren't published to the tap by default** (`dist`'s own
  default, not something set in `dist-workspace.toml`). A tag like
  `v0.2.0-rc.1` will still cut a GitHub Release with binaries, just without
  touching the Homebrew formula.
- The Homebrew-publish job needs push access to the separate tap repo, which
  the default `GITHUB_TOKEN` can't provide across repos. It authenticates
  with a PAT stored as the `HOMEBREW_TAP_TOKEN` secret on this repo — if that
  token expires or is revoked, only the `publish-homebrew-formula` job fails;
  the GitHub Release itself still succeeds and can be re-run once a fresh
  token is in place (`gh run rerun <run-id> --failed`).
