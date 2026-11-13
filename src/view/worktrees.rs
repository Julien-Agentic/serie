use std::{path::PathBuf, rc::Rc};

use ratatui::{
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Padding},
    Frame,
};

use crate::{
    app::AppContext,
    event::{AppEvent, Sender, UserEvent, UserEventWithCount},
    git::Worktree,
    view::{ListRefreshViewContext, RefreshViewContext},
    widget::commit_list::{CommitList, CommitListState},
};

#[derive(Debug)]
pub struct WorktreesView<'a> {
    commit_list_state: Option<CommitListState<'a>>,
    worktrees: Vec<Worktree>,
    current_path: PathBuf,
    selection: ListState,
    ctx: Rc<AppContext>,
    tx: Sender,
}

impl<'a> WorktreesView<'a> {
    pub fn new(
        commit_list_state: CommitListState<'a>,
        worktrees: Vec<Worktree>,
        current_path: PathBuf,
        ctx: Rc<AppContext>,
        tx: Sender,
    ) -> Self {
        let worktrees: Vec<_> = worktrees
            .into_iter()
            .filter(|w| !w.bare)
            .map(|mut w| {
                w.path = std::fs::canonicalize(&w.path).unwrap_or(w.path);
                w
            })
            .collect();
        let selected = worktrees
            .iter()
            .position(|w| w.path == current_path)
            .or_else(|| (!worktrees.is_empty()).then_some(0));
        Self {
            commit_list_state: Some(commit_list_state),
            worktrees,
            current_path,
            selection: ListState::default().with_selected(selected),
            ctx,
            tx,
        }
    }

    pub fn handle_event(&mut self, event: UserEventWithCount, _: KeyEvent) {
        match event.event {
            UserEvent::Quit => self.tx.send(AppEvent::Quit),
            UserEvent::Cancel | UserEvent::Close | UserEvent::WorktreeList => {
                self.tx.send(AppEvent::CloseWorktrees);
            }
            UserEvent::RefList => self.tx.send(AppEvent::OpenRefs),
            UserEvent::Confirm | UserEvent::SwitchBranch => {
                if let Some(worktree) = self.selected_worktree() {
                    self.tx.send(AppEvent::OpenWorktree(worktree.path.clone()));
                }
            }
            UserEvent::NavigateDown | UserEvent::SelectDown => {
                self.move_selection(true, event.count);
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                self.move_selection(false, event.count);
            }
            UserEvent::GoToTop => self.move_selection(false, usize::MAX),
            UserEvent::GoToBottom => self.move_selection(true, usize::MAX),
            UserEvent::ShortCopy | UserEvent::FullCopy | UserEvent::CopyBranchName => {
                if let Some(worktree) = self.selected_worktree() {
                    self.tx.send(AppEvent::CopyToClipboard {
                        name: "Worktree Path".into(),
                        value: worktree.path.to_string_lossy().into_owned(),
                    });
                }
            }
            UserEvent::HelpToggle => self.tx.send(AppEvent::OpenHelp),
            UserEvent::Refresh => self.refresh(),
            _ => {}
        }
    }

    fn move_selection(&mut self, down: bool, count: usize) {
        if let Some(index) = self.selection.selected() {
            let next = if down {
                index
                    .saturating_add(count)
                    .min(self.worktrees.len().saturating_sub(1))
            } else {
                index.saturating_sub(count)
            };
            self.selection.select(Some(next));
        }
    }

    fn selected_worktree(&self) -> Option<&Worktree> {
        self.worktrees.get(self.selection.selected()?)
    }

    pub fn select_path(&mut self, path: &std::path::Path) {
        if let Some(index) = self.worktrees.iter().position(|w| w.path == path) {
            self.selection.select(Some(index));
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let [list_area, panel_area] = self.split_areas(area);
        f.render_stateful_widget(
            CommitList::new(self.ctx.clone()),
            list_area,
            self.as_mut_list_state(),
        );
        let items: Vec<_> = self
            .worktrees
            .iter()
            .map(|worktree| {
                let marker = if worktree.path == self.current_path {
                    "* "
                } else {
                    "  "
                };
                ListItem::new(vec![
                    Line::raw(format!(
                        "{marker}{}",
                        worktree.branch.as_deref().unwrap_or("(detached HEAD)")
                    )),
                    Line::raw(format!("  {}", worktree.path.display())),
                ])
            })
            .collect();
        let panel = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(self.ctx.color_theme.divider_fg))
                    .title(" Worktrees ")
                    .padding(Padding::horizontal(1)),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(panel, panel_area, &mut self.selection);
    }

    fn split_areas(&self, area: Rect) -> [Rect; 2] {
        let graph_width = self.as_list_state().graph_area_cell_width() + 1;
        let width = area
            .width
            .saturating_sub(graph_width)
            .min(self.ctx.ui_config.refs.width);
        Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(area)
    }

    pub fn update_layout(&mut self, area: Rect) {
        let [list_area, _] = self.split_areas(area);
        self.as_mut_list_state()
            .update_height(list_area.height as usize);
    }

    pub fn take_list_state(&mut self) -> CommitListState<'a> {
        self.commit_list_state.take().unwrap()
    }

    fn as_list_state(&self) -> &CommitListState<'a> {
        self.commit_list_state.as_ref().unwrap()
    }

    fn as_mut_list_state(&mut self) -> &mut CommitListState<'a> {
        self.commit_list_state.as_mut().unwrap()
    }

    pub fn prepare_graph_uploads(&mut self) {
        self.as_mut_list_state().ensure_visible_graph_uploaded();
    }

    pub fn drain_pending_graph_uploads(&mut self) -> Vec<String> {
        self.as_mut_list_state().drain_pending_graph_uploads()
    }

    pub fn graph_image_ids_sorted(&self) -> Vec<u32> {
        self.as_list_state().graph_image_ids_sorted()
    }

    pub fn refresh_context(&self) -> RefreshViewContext {
        RefreshViewContext::Worktrees {
            list_context: ListRefreshViewContext::from(self.as_list_state()),
            selected_path: self.selected_worktree().map(|w| w.path.clone()),
        }
    }

    pub fn refresh(&self) {
        self.tx.send(AppEvent::Refresh(self.refresh_context()));
    }
}
