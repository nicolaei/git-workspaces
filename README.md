# git-workspaces

A manifest-driven multi-repo git plugin. `git workspaces <cmd>` manages a fleet
of independent git repos declared in a `workspaces.toml` at the root of a
parent folder — the pattern you want when several sibling repos live under
one folder and get worked on together.

Status: walking skeleton. The binary builds and responds to
`git workspaces --version` / `--help`. No manifest support or repo operations
yet — those land in later stories.

## Local install

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
