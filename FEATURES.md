# Features

Feature inventory collected during the Serie exploration session.

## Goal

Replace the need to keep VS Code open solely for Git Graph with a lightweight,
graph-first tool for following AI agents' work. Preserve Serie's visual graph
rendering and navigation, adding only the controls we actually need.

The agreed approach is to evaluate stock Serie first, then maintain a small fork.
A full Git client is explicitly out of scope for now.

## Status

- **Existing:** available in stock Serie.
- **Configured:** enabled during this session through local configuration.
- **Implemented:** added in this fork.
- **Proposed:** discussed for the fork; not implemented.
- **Considered:** a possible follow-up, not an agreed requirement.

The configurations made during this session live outside this repository. The
fork is this repository: upstream Serie is the `upstream` remote and fork work
happens on feature branches.

## Existing Serie features to preserve

### Graph-first history browsing — Existing

- Graphical branch and merge rendering using terminal image protocols.
- Navigate commits with arrow keys or `j` / `k`.
- Open commit details and a changed-file summary with `Enter`.
- Browse branch and tag references with `Tab`.
- Search with `/` and inspect available controls with `?`.

### Manual refresh — Existing

- Refresh repository history with `R` after an agent creates commits or changes
  references.

### Configurable external commands — Existing

- Bind external tools to shortcuts.
- Pass the selected commit hash to a command.
- Display command output inside Serie or temporarily suspend Serie for an
  interactive application.
- Refresh after commands that change repository state.

## Configured during this session

### Delta diff rendering — Configured

- Keep `d` as the shortcut for the selected commit's diff.
- Render syntax-highlighted diffs with `git-delta` inside Serie's existing pane.
- Use side-by-side layout with unlimited line wrapping.
- Size the output to the pane width, accounting for Serie's horizontal padding.
- Let Serie handle scrolling instead of launching a separate pager.
- Show root-commit changes and compare merge commits with their first parent.
- Leave the global Git pager setting unchanged.

Configuration: `~/.config/serie/config.toml`.

**Limitation:** Delta does not wrap unified diffs in the current inline setup,
and Serie's diff pane clips long lines rather than wrapping them. Side-by-side
rendering is the configuration-only workaround currently enabled.

### Hunk commit review — Configured

- Press `Shift-D` to open the selected commit in Hunk.
- Temporarily give Hunk control of the terminal; quit with `q` to return to Serie.
- Navigate changed files using Hunk's selectable sidebar.
- Press `e` to open the selected file in the configured editor.
- Keep this workflow alongside Delta rather than replacing `d`.

Configuration: `~/.config/serie/config.toml`.

**Limitations:** Hunk's sidebar navigates a continuous multi-file diff stream.
Opening a file in the editor opens its current on-disk version, not the historical
version from the selected commit. A file that no longer exists cannot be opened
through that action.

### Neovim as the default editor — Configured

- Set `EDITOR` and `VISUAL` to `nvim` in Fish.
- Set Git's `core.editor` to `nvim`.
- Use Neovim for Hunk's `e` action.

Configuration: `~/.config/fish/config.fish` and `~/.gitconfig`.
Existing shells and running applications need their environment updated or a
restart to pick up the editor change.

## Proposed additions to the fork

### Branch picker and switching — Implemented

On the `branch-switching` branch. The primary control follows LazyGit: put the
cursor on a commit and press `Space`.

In the commit list:

- `Space` switches to the branch whose head is the selected commit.
- Several branches on that commit: the status line lists them and a digit picks
  one; any other key cancels.
- A remote branch with no local branch creates a tracking local branch. If its
  local branch exists at another commit, nothing happens and a message says so.
- A commit with no branch is left alone; HEAD is never detached.
- `Space` no longer pages the commit list (`Ctrl-f` and `PageDown` still do). It
  still pages down in the commit detail, user command and help views.

In the refs list (`Tab`), which is stock Serie's side panel:

- `Space` on a branch switches to it.
- The current branch is marked `*`, a branch checked out in another worktree `+`.
- An earlier filter (`/`) and `Enter`-to-switch were removed once `Space` became
  the main control.

Safety, wherever the switch starts:

- A branch checked out in another worktree is refused, with that worktree's path.
- Uncommitted changes to tracked files ask for a y/N confirmation first.
- Nothing is forced, stashed or discarded; git's own refusal is shown as-is.

**Not done:** offering to open the other worktree instead. That needs the
worktree picker.

### Copy branch name — Existing

- Stock Serie already copies the selected branch or tag name with `c` in the
  refs list (`Tab`). This was listed as proposed by mistake.

### Branch creation — Proposed

- Prompt for a new branch name.
- Explicitly choose HEAD or the selected commit as the starting point.
- Offer an option to switch to the new branch after creating it.

### Branch deletion — Proposed

- Delete a local branch, its remote counterpart, or both.
- Confirm before deleting; never delete a branch checked out in a worktree.
- Added after the evaluation; safeguards are still open in
  [FRICTION.md](FRICTION.md).

### Worktree picker — Proposed

- Show worktrees in a second right-hand side panel on its own key, `w`, next
  to the refs list on `Tab`.
- List existing worktrees with their branch and directory.
- Select a worktree without leaving Serie.
- Change Serie's repository context, not the branch checked out in that worktree.
- Ensure repository reads, diffs, external commands, and editor actions all target
  the selected worktree consistently.

### Selectable changed-file list inside Serie — Proposed

- Make files in the commit detail view individually selectable.
- Add a direct open-in-editor action, potentially bound to `e`.
- Use the configured editor without requiring a round trip through Hunk.
- Clearly distinguish opening a working-tree file from viewing historical content.

Stock Serie currently shows a changed-file summary, but has neither a selectable
file list nor a built-in open-file shortcut.

### Adjustable graph column width — Implemented

- `<` narrows the graph column and `>` widens it, in the commit list. Both take
  a count, as in `3<`.
- Narrowing hides one lane from the right per press, until one is left. The
  lanes that stay keep their size: the graph is cropped, never rescaled.
  Widening shows one more lane, as far as the terminal has room for.
- The status line reports the result, for example `Graph: 4 of 6 lanes`.
- The chosen width survives refreshes and branch switches. If the graph later
  outgrows the terminal, it is narrowed to fit.
- The number of lanes shown is saved per repository and restored at the next
  launch. It lives in the repository's git config as `serie.graphLanes`, so all
  worktrees of the repository share it. Showing every lane again removes it.
- A commit in a hidden lane shows no node on its row.

**Not done:** narrowing automatically at startup when nothing is saved. Stock
Serie still exits with "Terminal too small" when the full graph does not fit at
launch.

### Unified diff wrapping — Proposed

- Wrap long lines directly in Serie's inline diff pane.
- Preserve the unified diff layout for users who prefer it over side-by-side.
- Keep scrolling correct when one logical line occupies multiple screen rows.

### Automatic refresh for agent activity — Proposed

- Detect externally created commits and reference changes.
- Update the graph without repeatedly pressing `R`.
- Preserve the selected commit and browsing position where possible, rather than
  jumping around as agents work.

### Safe behavior around active agents — Proposed

- Never force a checkout, automatically stash, or discard changes.
- Surface Git errors clearly instead of silently working around them.
- Prefer opening an existing worktree over changing its checked-out branch.
- Avoid switching branches underneath an agent working in that checkout.

## Considered, but not scoped

- Visibility into uncommitted changes across multiple agent worktrees. The shared
  commit graph alone cannot provide this; each worktree has its own working state.
- Opening a historical version of a file in an editor, distinct from editing the
  current working-tree copy.

## Evaluation criteria before expanding the fork

- Graph readability at normal terminal and split-pane sizes.
- Fast, comfortable commit navigation and diff review.
- Correct branch and HEAD context when launched from a worktree.
- Stable selection during refreshes.
- Compatibility with the terminal and multiplexer actually used day to day.
- Measured memory and CPU usage compared with the existing VS Code workflow.

The friction list from the evaluation is in [FRICTION.md](FRICTION.md), separating
**must fix**, **nice to have**, and **unfamiliar controls**. Implement one
high-impact feature at a time, in the build order recorded there. The worktree
picker was originally suggested as the first fork prototype; the evaluation moved
branch switching ahead of it.
