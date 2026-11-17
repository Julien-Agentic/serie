# Friction list

Outcome of the hands-on evaluation of stock Serie 0.10.1, recorded on 2026-11-11.
It decides the order of fork work. Feature descriptions live in
[FEATURES.md](FEATURES.md).

## Must fix

These block replacing VS Code's Git Graph.

1. **Branch switching.** No searchable branch picker and no way to check out a
   branch from inside Serie. *Fixed on the `branch-switching` branch:* `Space`
   on a commit switches to the branch whose head it is, as in LazyGit.
2. **Branch deletion, local and remote.** *Fixed:* `d` safely deletes and `D`
   explicitly force-deletes local branches, with confirmation. Remote deletion
   is a separate confirmed action, protected by a lease. Current and worktree-held
   branches are refused. Delta/Hunk review moved to `v` / `V`.
3. **Selectable changed-file list.** *Fixed:* `Tab` focuses the changed-file list
   in commit details; `j` / `k` selects and `e` edits the current working-tree file
   using `$VISUAL` / `$EDITOR`, without a Hunk round trip.

## Nice to have

- **Worktree picker.** *Fixed:* `w` opens a right-hand worktree panel;
  `Enter` / `Space` changes Serie's context without checking out a branch.
  Repository reads and external commands follow the selected worktree.
- **Copy branch name from a commit.** *Fixed:* `C` copies a branch name in
  the commit list and detail view, preferring local branches and offering a
  digit picker when needed. `c` still copies the short SHA. In the refs list,
  both `c` and `C` copy the selected branch or tag name.
- **Jump between branch tips.** *Fixed:* `[` and `]` move to the previous or
  next local or remote branch tip in display order, with counts such as `3]`.
  Navigation does not wrap or switch branches.
- **Automatic refresh.** `R` has to be pressed after every agent commit.
- **Adjustable graph column width.** *Fixed:* `<` and `>` narrow and widen the
  graph column.
- **Unified diff wrapping.** Long lines are clipped in the inline diff pane;
  side-by-side Delta remains the workaround.

## Unfamiliar controls

None reported.

## Build order

One feature at a time, in this order:

1. **Branch picker and switching — implemented.** It supplies the branch list
   and selection UI that deletion reuses.
2. **Branch deletion — implemented.**
3. **Selectable changed-file list — implemented,** with open-in-editor.

All original must-fix items are implemented. Remaining follow-ups include automatic
refresh, unified diff wrapping, and automatic graph narrowing at startup; see
FEATURES.md.

This replaces the earlier suggestion in FEATURES.md to prototype the worktree
picker first. Branch switching is now worktree-aware: a branch checked out in
another worktree offers to open that checkout with `y/N`, never force-checks it
out here. Worktree ownership is re-read before offering and accepting.

## Deletion safeguards

Deletion is the first destructive action in the fork and has to fit the "safe
behaviour around active agents" rules. Agreed on 2026-11-11:

- Confirm before every deletion, with a separate explicit confirmation for the
  remote branch.
- Refuse to delete the current branch or a branch checked out in any worktree.
- Use `d` for safe deletion and show Git's "not fully merged" error. Force
  deletion is a separate explicit `D` action with its own confirmation.
- Re-read worktrees and verify the selected branch tip before deleting. Remote
  deletion uses a lease and also protects active local branches tracking it.

Implemented in the fork; see [FEATURES.md](FEATURES.md) for the full behavior.
