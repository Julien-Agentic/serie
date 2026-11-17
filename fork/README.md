# Personal Serie fork

This fork keeps Serie's graph-first browsing experience and adds controls for
reviewing work across branches and agent worktrees. It is not a full Git client.
Upstream remains [lusingander/serie](https://github.com/lusingander/serie).

## Features

Built into this fork:

- Branch switching with `Space`, branch-name copying with `C`, and confirmed
  local/remote branch deletion with `d` / `D`.
- Worktree browsing with `w`, without changing either worktree's checked-out branch.
- Branch-tip navigation with `[` / `]`, first-parent lineage with `{` / `}`,
  and graph column cropping with `<` / `>`.
- Selectable changed files in commit details (`Tab`), with `e` to edit the
  **current working-tree file**, not historical content.
- Inline diff file/hunk navigation and faster handoff to interactive tools.

See [FEATURES.md](FEATURES.md) for behavior and limitations,
[FRICTION.md](FRICTION.md) for remaining work, and the
[application keybindings](../docs/src/keybindings/index.md) for controls.

Delta rendering, Hunk review, and fetch shortcuts are optional configuration,
not built-in dependencies. The example below supplies them.

## Install on another machine

Requirements: Git, Rust/Cargo (Rust 1.88 or newer), and a terminal supporting
Serie's image protocols. See the upstream README's
[requirements](../README.md#requirements). Local validation so far is on macOS
arm64; other terminal/platform combinations still need hands-on testing.

```sh
git clone https://github.com/Julien-Agentic/serie.git
cd serie
cargo install --path . --locked
```

Ensure Cargo's binary directory (normally `~/.cargo/bin`) is on `PATH`. The
binary is still named `serie`; installation can replace an existing Cargo-installed
Serie. `cargo install --locked serie` installs the crates.io version, not this fork.

For an isolated trial without replacing an installed binary:

```sh
cargo build --release --locked
# Run from a repository you want to browse:
/path/to/serie/target/release/serie
```

After pulling updates with `git pull --ff-only`, rerun the installation command.
The package version is inherited from upstream; use `git rev-parse HEAD` to
identify the exact fork revision you installed.

## Optional configuration

[config.example.toml](config.example.toml) captures the portable parts of our
setup. It uses executables on `PATH`, not machine-specific paths:

| Tool | Used for |
| --- | --- |
| Bash and `delta` (git-delta) | `v`: side-by-side inline diff |
| `hunk` | `V`: interactive commit review; quit to return to Serie |
| Git | `f`: fetch; `F`: fetch all remotes and prune |
| `$VISUAL`, then `$EDITOR`, then `vi` | `e`: edit a selected working-tree file |

Install the optional tools you want and remove unused commands/keybindings from
the example. Set your editor in your shell if desired; Neovim is not required.
These settings do not change your global Git pager or editor configuration.

Try the example without changing your existing configuration (use an absolute
path when running from another repository):

```sh
SERIE_CONFIG_FILE=/absolute/path/to/serie/fork/config.example.toml serie
```

To keep it, copy it to `$XDG_CONFIG_HOME/serie/config.toml` (default:
`~/.config/serie/config.toml`) **only if you do not already have a config**.
Otherwise merge the relevant sections manually. An existing `SERIE_CONFIG_FILE`
environment variable takes precedence over that location.

The example also reduces list columns to graph, marker, and subject. Delta's
side-by-side layout is the current workaround for clipped long unified-diff lines.
Serie handles scrolling; Delta does not launch a pager.

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for branches, checks, and upstream proposals.
