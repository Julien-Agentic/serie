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
3. **Selectable changed-file list.** Files in the commit detail view cannot be
   selected or opened in the editor without a round trip through Hunk.

## Nice to have

- **Worktree picker.** No way to point Serie at another worktree without
  relaunching. Wanted as a second right-hand side panel on `w`.
- **Copy branch name from a commit.** *Fixed:* `C` copies a branch name in
  the commit list and detail view, preferring local branches and offering a
  digit picker when needed. `c` still copies the short SHA. In the refs list,
  both `c` and `C` copy the selected branch or tag name.
- **Jump between branch tips.** *Fixed:* `{` and `}` move to the previous or
  next local or remote branch tip in display order, with counts such as `3}`.
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
3. **Selectable changed-file list — next,** with open-in-editor. It is independent of
   the branch work and can move earlier if it hurts more in daily use.

This replaces the earlier suggestion in FEATURES.md to prototype the worktree
picker first. Branch switching still has to be worktree-aware: a branch checked
out in another worktree must be flagged, never force-checked-out.

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
