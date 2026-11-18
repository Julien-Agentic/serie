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

A portable version of the optional tool configuration is checked in as
[config.example.toml](config.example.toml); installed personal configuration stays
outside the repository. The complete fork lives on `main`, with `origin` pointing
to our fork and `upstream` to original Serie. See [DEVELOPMENT.md](DEVELOPMENT.md).

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

- Use `v` as the shortcut for the selected commit's diff (`d` is now deletion).
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

- Press `Shift-V` to open the selected commit in Hunk (`Shift-D` is now force deletion).
- Temporarily give Hunk control of the terminal; quit with `q` to return to Serie.
- Navigate changed files using Hunk's selectable sidebar.
- Press `e` to open the selected file in the configured editor.
- Keep this workflow alongside Delta rather than replacing `v`.

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

## Fork additions and follow-ups

### Faster interactive-tool handoff — Implemented

- Read terminal input on the application thread, so opening Hunk (`V`) or an
  editor (`e`) no longer waits up to 100 ms for a background input thread to stop.
- Launch user commands from cached commit metadata without first computing an
  unused changed-file summary through Git.
- A local release-build terminal probe measured median key-to-command handoff
  dropping from about 106 ms to 5 ms. This excludes Hunk's own startup and diff
  loading, which remain dependent on the tool and repository.

### Branch picker and switching — Implemented

The primary control follows LazyGit: put the cursor on a commit and press `Space`.

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

- For a branch checked out in another worktree, show its path and ask whether to
  open that worktree instead (`y/N`). Declining never falls back to checkout.
- Re-read worktrees before offering and accepting the prompt, including when a
  branch acquires a worktree during dirty-checkout confirmation. If the branch's
  worktree changes before acceptance, cancel and ask the user to select again.
- Uncommitted changes to tracked files ask for a y/N confirmation first.
- Nothing is forced, stashed or discarded; git's own refusal is shown as-is.

Opening an existing worktree uses the same context-switching path as the picker:
select its HEAD, run diffs and commands there, and never change either worktree's
branch or working files. Failed opens retain the originating commit list or refs
panel.

### Copy branch name — Implemented

- `C` copies a branch name at the selected commit in the commit list and detail
  view; `c` still copies the short SHA.
- Prefer local branches, including the current branch. With multiple locals,
  choose with a digit (up to nine); any other key cancels.
- Without a local branch, offer remote-tracking names, excluding remote HEAD
  aliases. With no branch, show a message and leave the clipboard unchanged.
- In the refs list (`Tab`), both `c` and `C` copy the selected branch or tag name.
- Full SHA copying remains available as the unbound `full_copy` action for custom
  keybindings.

### Keybind to jump to previous/next branch — Implemented

- `[` jumps upward to the previous branch tip; `]` jumps downward to the next.
- Follow displayed history order, including local and remote branches but not
  tags or stashes. Several branches on one commit are one stop.
- Skip the current commit and stop at either end without wrapping.
- Support counts, such as `3]`. Only selection changes; `Space` still switches
  branches. Brackets remain ordinary text while typing a search.
- `{` / `}` instead navigate first-parent lineage toward tips / the root,
  with a path chooser at forks.

### Branch deletion — Implemented

- `d` requests safe deletion; `D` requests force deletion of a local branch.
- Available in the commit list, commit details and refs panel. Multiple branches
  at one commit get a numbered local/REMOTE picker, with `n` for further pages.
- Confirm every action with `y`; any other key cancels. Safe local deletion uses
  `git branch -d`. If unmerged, show Git's error; `D` plus a new confirmation is
  the explicit force step (`git branch -D`).
- Remote deletion removes the server branch, with a separate confirmation and
  a lease protecting against a changed server tip. There is no remote force mode.
- Remote deletion never prompts for credentials or SSH passphrases; it relies on
  an agent or credential helper and otherwise fails with Git's error.
- Delete local and remote separately to remove both; neither implicitly deletes
  the other. Refresh after each successful deletion.
- Refuse current and worktree-held branches even with force. Re-read worktrees
  before deleting; remote deletion also protects active locals tracking it.
- Refuse branch tips changed since refresh. Never switch, stash or discard.
- Delta/Hunk review shortcuts moved to `v` / `V` to free `d` / `D`.
- Safeguards are recorded in [FRICTION.md](FRICTION.md).

### Worktree picker — Implemented

- `w` opens a right-hand worktree panel from the commit list or refs panel;
  `Tab` switches back to refs. The panels share the configured refs width.
- List existing non-bare worktrees with their branch (or detached HEAD) and
  directory; `*` marks the current worktree, even when launched in a subdirectory.
- Navigate with `j` / `k`, arrows, or `g` / `G`, including movement counts.
  `Enter` or `Space` opens the selected worktree and selects its HEAD.
- Change Serie's repository context and working directory, never the branch
  checked out in either worktree. Dirty worktrees require no checkout or stash.
- Repository reads, diffs, external commands, and editors launched by those
  commands all target the selected worktree.
- Validate the destination before switching; an unavailable worktree leaves the
  original context intact and shows an error. Locked worktrees can be browsed.
- `R` refreshes both history and worktrees, preserving panel selection by path.
  `c` / `C` copies the selected directory. `w`, `Esc`, or `Backspace` closes
  the picker without changing context.

### Selectable changed-file list inside Serie — Implemented

- In commit details, `Tab` toggles focus between text scrolling and changed files.
  With files focused, arrows / `j` / `k` select a file; counts, page/half-page
  movement, and `g` / `G` work without wrapping. Selection stays visible on resize.
- `e` opens the selected file in `$VISUAL`, then `$EDITOR`, falling back to `vi`.
  Editor arguments/quoting are supported; filenames are passed as literal arguments.
  Serie suspends and resumes around the editor, retaining the detail view.
- Open the **current working-tree version**, never historical content. Renames
  use the destination path. Missing/deleted files are not recreated. Directories,
  submodules, and symlinks resolving outside the worktree are refused.
- Always resolve paths relative to the selected worktree root, including when
  launched from a subdirectory. Spaces, tabs, and newlines in Git filenames are
  preserved; control characters are escaped only for display.
- `R` preserves the selected file by path, focus and scroll position. Changing
  commits with `J` / `K` or parent navigation resets selection to the first file.
- `Tab` again restores text scrolling; `Enter`, `Esc`, and `Backspace` still close
  details. `e` also works in text mode on the retained selection (first file by
  default). No Hunk round trip is needed.

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

### Automatic refresh for agent activity — Implemented

- Enable with `[core.option]` and `auto_refresh = true`. Checks Git's HEAD and
  refs every 10 seconds by default; set `auto_refresh_interval_secs = 5` for
  faster updates. `R` remains available for immediate refresh.
- Reload only when a snapshot changes. Preserve the selected commit and its
  screen row where possible, and defer while a prompt or inline command is open.

### Safe behavior around active agents — Proposed

- Never force a checkout, automatically stash, or discard changes.
- Surface Git errors clearly instead of silently working around them.
- Prefer opening an existing worktree over changing its checked-out branch.
  **Implemented:** branch switching offers the existing worktree with explicit
  confirmation, rather than attempting checkout.
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
