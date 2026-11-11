# Friction list

Outcome of the hands-on evaluation of stock Serie 0.10.1, recorded on 2026-11-11.
It decides the order of fork work. Feature descriptions live in
[FEATURES.md](FEATURES.md).

## Must fix

These block replacing VS Code's Git Graph.

1. **Branch switching.** No searchable branch picker and no way to check out a
   branch from inside Serie. *Fixed on the `branch-switching` branch:* `Space`
   on a commit switches to the branch whose head it is, as in LazyGit.
2. **Branch creation.** Cannot create a branch from HEAD or the selected commit.
3. **Branch deletion, local and remote.** Cannot delete a branch from inside
   Serie. This was not in the original feature inventory; it came out of the
   evaluation.
4. **Selectable changed-file list.** Files in the commit detail view cannot be
   selected or opened in the editor without a round trip through Hunk.

## Nice to have

- **Cursor highlight stops at the commit text.** The selection bar covers only
  the commit's text, so it is hard to see which graph node the cursor is on.
  Fix: extend the highlight bar across the graph column as well.
- **Worktree picker.** No way to point Serie at another worktree without
  relaunching. Wanted as a second right-hand side panel on `w`.
- ~~**Copy branch name.**~~ Not a friction: stock Serie already does this with
  `c` in the refs list.
- **Automatic refresh.** `R` has to be pressed after every agent commit.
- **Adjustable graph column width.** *Fixed:* `<` and `>` narrow and widen the
  graph column.
- **Unified diff wrapping.** Long lines are clipped in the inline diff pane;
  side-by-side Delta remains the workaround.

## Unfamiliar controls

None reported.

## Build order

One feature at a time, in this order:

1. **Branch picker and switching.** It supplies the branch list and selection
   UI that creation and deletion both reuse.
2. **Branch creation.**
3. **Branch deletion.**
4. **Selectable changed-file list** with open-in-editor. It is independent of
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
- Use a safe delete first and show Git's "not fully merged" error; require a
  second, explicit step to force.
