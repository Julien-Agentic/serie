# Keybindings

You can see the keybindings by pressing the `?` key.

The default key bindings can be overridden.

## List of all default keybindings

#### Common

| Key                            | Description | Corresponding keybind |
| ------------------------------ | ----------- | --------------------- |
| <kbd>Ctrl-c</kbd> <kbd>q</kbd> | Quit app    | `force_quit` `quit`   |
| <kbd>?</kbd> <kbd>F1</kbd>     | Open help   | `help_toggle`         |

#### Commit List

| Key                                  | Description                                        | Corresponding keybind                        |
| ------------------------------------ | -------------------------------------------------- | -------------------------------------------- |
| <kbd>Down/Up</kbd> <kbd>j/k</kbd>    | Move down/up                                       | `navigate_down` `navigate_up`                |
| <kbd>J/K</kbd>                       | Move down/up                                       | `select_down` `select_up`                    |
| <kbd>Alt-Down</kbd> <kbd>Alt-j</kbd> | Move to parent commit                              | `go_to_parent`                               |
| <kbd>g/G</kbd>                       | Go to top/bottom                                   | `go_to_top` `go_to_bottom`                   |
| <kbd>Ctrl-f/b</kbd>                  | Scroll page down/up                                | `page_down` `page_up`                        |
| <kbd>Ctrl-d/u</kbd>                  | Scroll half page down/up                           | `half_page_down` `half_page_up`              |
| <kbd>Ctrl-e/y</kbd>                  | Scroll down/up                                     | `scroll_down` `scroll_up`                    |
| <kbd>H/M/L</kbd>                     | Select top/middle/bottom of the screen             | `select_top` `select_middle` `select_bottom` |
| <kbd>Enter</kbd>                     | Show commit details<br>Apply search (if searching) | `confirm`                                    |
| <kbd>Tab</kbd>                       | Open refs list                                     | `ref_list`                                   |
| <kbd>w</kbd>                         | Open worktree picker                               | `worktree_list`                              |
| <kbd>Space</kbd>                     | Switch to the branch at the selected commit        | `switch_branch`                              |
| <kbd>{/}</kbd>                       | Go to previous/next branch tip                     | `go_to_previous_branch` `go_to_next_branch`  |
| <kbd>&lt;/&gt;</kbd>                 | Narrow/widen the graph column                      | `graph_narrower` `graph_wider`               |
| <kbd>/</kbd>                         | Start search                                       | `search`                                     |
| <kbd>Esc</kbd>                       | Cancel search                                      | `cancel`                                     |
| <kbd>n/N</kbd>                       | Go to next/previous search match                   | `go_to_next` `go_to_previous`                |
| <kbd>Ctrl-t</kbd>                    | Toggle search target                               | `search_target_toggle`                       |
| <kbd>Ctrl-g</kbd>                    | Toggle ignore case                                 | `ignore_case_toggle`                         |
| <kbd>Ctrl-x</kbd>                    | Toggle fuzzy match                                 | `fuzzy_toggle`                               |
| <kbd>m</kbd>                         | Toggle commit mark                                 | `mark_toggle`                                |
| <kbd>R</kbd>                         | Refresh                                            | `refresh`                                    |
| <kbd>c</kbd>                         | Copy commit short hash                             | `short_copy`                                |
| <kbd>C</kbd>                         | Copy branch name                                   | `copy_branch_name`                          |
| <kbd>d/D</kbd>                       | Delete/force-delete a branch (confirm first)        | `delete_branch` `force_delete_branch`       |
| <kbd>v</kbd>                         | Toggle custom user command view                    | `user_command_1`                             |

#### Commit Detail

| Key                                  | Description                     | Corresponding keybind           |
| ------------------------------------ | ------------------------------- | ------------------------------- |
| <kbd>Esc</kbd> <kbd>Backspace</kbd>  | Close commit details            | `close` `cancel`                |
| <kbd>Down/Up</kbd> <kbd>j/k</kbd>    | Scroll down/up                  | `navigate_down` `navigate_up`   |
| <kbd>Ctrl-f/b</kbd>                  | Scroll page down/up             | `page_down` `page_up`           |
| <kbd>Ctrl-d/u</kbd>                  | Scroll half page down/up        | `half_page_down` `half_page_up` |
| <kbd>g/G</kbd>                       | Go to top/bottom                | `go_to_top` `go_to_bottom`      |
| <kbd>J/K</kbd>                       | Select older/newer commit       | `select_down` `select_up`       |
| <kbd>Alt-Down</kbd> <kbd>Alt-j</kbd> | Select parent commit            | `go_to_parent`                  |
| <kbd>m</kbd>                         | Toggle commit mark              | `mark_toggle`                   |
| <kbd>R</kbd>                         | Refresh                         | `refresh`                       |
| <kbd>c</kbd>                         | Copy commit short hash          | `short_copy`                   |
| <kbd>C</kbd>                         | Copy branch name                | `copy_branch_name`             |
| <kbd>d/D</kbd>                       | Delete/force-delete a branch (confirm first) | `delete_branch` `force_delete_branch` |
| <kbd>v</kbd>                         | Toggle custom user command view | `user_command_1`                |

#### Refs List

| Key                                                | Description      | Corresponding keybind            |
| -------------------------------------------------- | ---------------- | -------------------------------- |
| <kbd>Esc</kbd> <kbd>Backspace</kbd> <kbd>Tab</kbd> | Close refs list  | `close` `cancel` `ref_list`      |
| <kbd>Down/Up</kbd> <kbd>j/k</kbd>                  | Move down/up     | `navigate_down` `navigate_up`    |
| <kbd>J/K</kbd>                                     | Move down/up     | `select_down` `select_up`        |
| <kbd>g/G</kbd>                                     | Go to top/bottom | `go_to_top` `go_to_bottom`       |
| <kbd>Right/Left</kbd> <kbd>l/h</kbd>               | Open/Close node  | `navigate_right` `navigate_left` |
| <kbd>Space</kbd>                                   | Switch to the selected branch | `switch_branch`     |
| <kbd>d/D</kbd>                                     | Delete/force-delete selected branch (confirm first) | `delete_branch` `force_delete_branch` |
| <kbd>R</kbd>                                       | Refresh          | `refresh`                        |
| <kbd>c/C</kbd>                                     | Copy ref name    | `short_copy` `copy_branch_name`  |
| <kbd>w</kbd>                                       | Open worktree picker | `worktree_list`               |

The current branch is marked with `*` and a branch checked out in another
worktree with `+`, as in `git branch`. Switching never forces, stashes or
discards: a branch held by another worktree is refused, uncommitted changes
ask for confirmation first, and anything git refuses is shown as an error.

In the commit list, <kbd>Space</kbd> switches to the branch whose head is the
selected commit. If several branches point at that commit, the status line
lists them and a digit picks one; any other key cancels. A remote branch
without a local branch is offered too and creates a tracking local branch.
A commit with no branch is left alone: Serie never detaches HEAD. In the
commit detail, user command and help views, <kbd>Space</kbd> scrolls a page
down as before.

### Worktree picker

Press <kbd>w</kbd> from the commit list or refs panel to list worktrees in a
right-hand panel. Each entry shows its branch (or detached HEAD) and directory;
`*` marks the current worktree. Bare repositories are not selectable.

| Key | Description | Corresponding keybind |
| --- | --- | --- |
| <kbd>j/k</kbd> <kbd>Down/Up</kbd> <kbd>J/K</kbd> | Select worktree (supports counts) | `navigate_down` `navigate_up` `select_down` `select_up` |
| <kbd>g/G</kbd> | First/last worktree | `go_to_top` `go_to_bottom` |
| <kbd>Enter</kbd> <kbd>Space</kbd> | Open selected worktree | `confirm` `switch_branch` |
| <kbd>w</kbd> <kbd>Esc</kbd> <kbd>Backspace</kbd> | Close picker | `worktree_list` `cancel` `close` |
| <kbd>Tab</kbd> | Open refs instead | `ref_list` |
| <kbd>R</kbd> | Refresh history and worktrees | `refresh` |
| <kbd>c/C</kbd> | Copy selected directory | `short_copy` `copy_branch_name` |

Opening a worktree changes Serie's context and working directory, then selects
that worktree's HEAD. It never switches branches, stashes or discards changes.
Diffs, user commands, and editors launched by those commands run in the selected
worktree. If it cannot be opened, the original context remains active and a
message explains why. Refreshing the picker retains its selection by path.
The panel uses the `ui.refs.width` setting.

### Branch deletion

In the commit list and detail view, <kbd>d</kbd> deletes a branch at the selected
commit. If several branches point there, choose a numbered **local** or **REMOTE**
entry; <kbd>n</kbd> cycles pages when there are more than nine choices. In the
refs panel, it targets the selected branch leaf, not a folder or tag.

Every deletion asks for <kbd>y</kbd> confirmation; any other key cancels. A local
delete uses `git branch -d`. If Git refuses an unmerged branch, nothing is deleted:
use <kbd>D</kbd> and confirm again to explicitly force-delete that local branch
with `git branch -D`. <kbd>D</kbd> does not force-delete remote branches.

Deleting a **REMOTE** entry removes the branch **from the server**, not merely
its remote-tracking reference. It requires its own explicit confirmation and
leaves local branches alone. To delete both local and remote, perform and confirm
the two actions separately. Remote deletion uses a lease and refuses if the
server tip no longer matches the loaded remote-tracking reference; refresh or
fetch and review before retrying.
Serie never prompts for credentials, SSH passphrases or host keys during a
remote deletion: set up an SSH agent or a credential helper, otherwise the
push fails and the error is shown.

Deletion refuses the current branch and branches checked out in any worktree,
even in force mode. Remote deletion also protects active local branches that
track it. Worktrees are re-read immediately before deletion, and branch tips
must still match the last refresh. Nothing is switched, stashed or discarded.
Git failures are shown on the status line; successful deletion refreshes the view.

Diff review moved from <kbd>d</kbd> to <kbd>v</kbd> (`user_command_1`). Custom
<kbd>D</kbd> review bindings should move to <kbd>V</kbd> (`user_command_2`),
otherwise they override the new force-delete binding.

### Copying and navigation

In the commit list and detail view, <kbd>C</kbd> copies a local branch name
at the selected commit, including the current branch. If several local branches
point there, choose one with a digit (up to nine choices); another key cancels.
If there are no local branches, remote-tracking names such as `origin/topic`
are offered instead, excluding `origin/HEAD`. With no branch, nothing is copied.
<kbd>c</kbd> still copies the short SHA. Full SHA copying remains available
through the `full_copy` action, which has no default binding. In the refs panel,
<kbd>c</kbd> and <kbd>C</kbd> both copy the selected branch or tag name.

In the commit list, <kbd>{</kbd> jumps upward to the previous branch tip and
<kbd>}</kbd> downward to the next, in the displayed history order. Local and
remote branches count; tags and stashes do not. Several branches at one commit
are one stop. The current commit is skipped, and navigation stops at either
end without wrapping. Counts work, for example <kbd>3}</kbd>. These keys only
move selection; use <kbd>Space</kbd> to switch branches. While typing a search,
braces are ordinary text.

<kbd>&lt;</kbd> narrows the graph column by hiding one lane from the right,
until one is left; the lanes that stay keep their size. <kbd>&gt;</kbd> shows
one more lane, as far as the terminal has room for. Both take a count, as in
<kbd>3</kbd><kbd>&lt;</kbd>. The number of lanes shown is remembered per
repository, in its git config as `serie.graphLanes`, and shared by all of its
worktrees.

#### User Command

| Key                                  | Description                 | Corresponding keybind           |
| ------------------------------------ | --------------------------- | ------------------------------- |
| <kbd>Esc</kbd> <kbd>Backspace</kbd>  | Close user command          | `close` `cancel`                |
| <kbd>Down/Up</kbd> <kbd>j/k</kbd>    | Scroll down/up              | `navigate_down` `navigate_up`   |
| <kbd>J/K</kbd>                       | Scroll down/up              | `select_down` `select_up`       |
| <kbd>Ctrl-f/b</kbd>                  | Scroll page down/up         | `page_down` `page_up`           |
| <kbd>Ctrl-d/u</kbd>                  | Scroll half page down/up    | `half_page_down` `half_page_up` |
| <kbd>g/G</kbd>                       | Go to top/bottom            | `go_to_top` `go_to_bottom`      |
| <kbd>J/K</kbd>                       | Select older/newer commit   | `select_down` `select_up`       |
| <kbd>Alt-Down</kbd> <kbd>Alt-j</kbd> | Select parent commit        | `go_to_parent`                  |
| <kbd>m</kbd>                         | Toggle commit mark          | `mark_toggle`                   |
| <kbd>R</kbd>                         | Refresh                     | `refresh`                       |

#### Help

| Key                                                            | Description              | Corresponding keybind           |
| -------------------------------------------------------------- | ------------------------ | ------------------------------- |
| <kbd>Esc</kbd> <kbd>Backspace</kbd> <kbd>?</kbd> <kbd>F1</kbd> | Close help               | `close` `cancel` `help_toggle`  |
| <kbd>Down/Up</kbd> <kbd>j/k</kbd>                              | Scroll down/up           | `navigate_down` `navigate_up`   |
| <kbd>J/K</kbd>                                                 | Scroll down/up           | `select_down` `select_up`       |
| <kbd>Ctrl-f/b</kbd>                                            | Scroll page down/up      | `page_down` `page_up`           |
| <kbd>Ctrl-d/u</kbd>                                            | Scroll half page down/up | `half_page_down` `half_page_up` |
| <kbd>g/G</kbd>                                                 | Go to top/bottom         | `go_to_top` `go_to_bottom`      |

----

- [Custom Keybindings](./custom-keybindings.md)
