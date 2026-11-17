# Developing the fork

## Remotes and branches

- `origin`: https://github.com/Julien-Agentic/serie — our fork.
- `upstream`: https://github.com/lusingander/serie — original Serie.
- `main`: the complete, usable fork; tracks `origin/main`.
- `feat/*` and `fix/*`: short-lived work based on `main`.
- `upstream/*`: isolated contribution branches based on `upstream/master`.

Do not rebase or force-push shared `main`. Keep it buildable and merge tested
features into it. Push unfinished feature branches when moving between machines;
uncommitted files and local configuration do not travel with Git.

A fresh clone needs the upstream remote added once:

```sh
git remote add upstream https://github.com/lusingander/serie.git
```

Start a feature:

```sh
git switch main
git pull --ff-only
git switch -c feat/automatic-refresh
# Implement, test, and commit.
git push -u origin feat/automatic-refresh
```

Open a pull request against this fork's `main`, not upstream's `master`.
After merging, update `main` on each machine with `git pull --ff-only`.

## Checks

Run from the repository root:

```sh
cargo fmt --all -- --check
cargo sort-derives --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked
```

Install the derive checker once with `cargo install --locked cargo-sort-derives`.
The test suite covers Git operations using temporary repositories and graph
snapshots. Interactive terminal behavior still needs manual testing: graph
rendering, navigation, resizing, worktree context switching, and returning from
an editor or Hunk. Do not commit generated `target/` or `out/` contents.

The regular Build workflow runs Linux checks. For hosted Apple Silicon coverage,
run **Actions → Build Apple Silicon → Run workflow** and select the branch to
check. This workflow runs on demand so macOS runner availability does not block
regular CI. On an Apple Silicon machine, the equivalent build check is
`cargo build --locked --target aarch64-apple-darwin`.

Changes to commands, defaults, and keybindings should update application docs,
`assets/default-keybind.toml`, `config.schema.json`, and tests as applicable.
Update `fork/config.example.toml` when our optional external-tool setup changes.
A regression test parses and validates that example through Serie's config types.

## Bring in upstream updates

With a clean working tree:

```sh
git fetch upstream
git switch main
git pull --ff-only
git merge upstream/master
# Resolve conflicts, run checks, and inspect behavior before publishing.
git push origin main
```

Keep upstream source layout, package identity, and release machinery intact.
Fork-specific documentation lives here; actual application usage documentation
continues to live in `docs/`. Do not publish upstream-style release tags unless
we deliberately set up fork releases.

## Propose changes upstream

Read [CONTRIBUTING.md](../CONTRIBUTING.md) first. Upstream requests discussion
before new features, behavior/default changes, or architectural changes, and may
consider Git-client operations outside its scope. Agreement on an idea is not
acceptance of our implementation.

```sh
git fetch upstream
git switch -c upstream/graph-width upstream/master
# Cherry-pick or adapt only the agreed change, including tests and docs.
git push -u origin upstream/graph-width
```

Review the diff against `upstream/master` before opening a PR explicitly targeting
`lusingander/serie:master`. Each proposal must work without the rest of the fork.
Do not send fork configuration, evaluation notes, or unrelated cleanup upstream.
The existing cumulative history is preserved; extraction happens per proposal.
