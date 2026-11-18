#[cfg(test)]
mod lineage_tests;

use std::rc::Rc;

use ratatui::{crossterm::event::KeyEvent, layout::Rect, Frame};

use crate::{
    app::AppContext,
    event::{AppEvent, Sender, UserEvent, UserEventWithCount},
    git::CommitHash,
    view::{ListRefreshViewContext, RefreshViewContext},
    widget::commit_list::{CommitList, CommitListState, LineageChoice, SearchState},
};

#[derive(Debug)]
struct LineagePicker {
    choices: Vec<LineageChoice>,
    selected: usize,
}

#[derive(Debug)]
pub struct ListView<'a> {
    commit_list_state: Option<CommitListState<'a>>,
    lineage_picker: Option<LineagePicker>,

    ctx: Rc<AppContext>,
    tx: Sender,
}

impl<'a> ListView<'a> {
    pub fn new(
        commit_list_state: CommitListState<'a>,
        ctx: Rc<AppContext>,
        tx: Sender,
    ) -> ListView<'a> {
        ListView {
            commit_list_state: Some(commit_list_state),
            lineage_picker: None,
            ctx,
            tx,
        }
    }

    pub fn handle_event(&mut self, event_with_count: UserEventWithCount, key: KeyEvent) {
        let event = event_with_count.event;
        let count = event_with_count.count;
        if self.lineage_picker.is_some() {
            self.handle_lineage_picker(event);
            return;
        }
        if let SearchState::Searching { .. } = self.as_list_state().search_state() {
            match event {
                UserEvent::Confirm => {
                    self.as_mut_list_state().apply_search();
                    self.update_matched_message();
                }
                UserEvent::Cancel => {
                    self.as_mut_list_state().cancel_search();
                    self.clear_search_query();
                }
                UserEvent::SearchTargetToggle => {
                    self.as_mut_list_state().toggle_search_target();
                    self.update_search_status();
                }
                UserEvent::IgnoreCaseToggle => {
                    self.as_mut_list_state().toggle_ignore_case();
                    self.update_search_status();
                }
                UserEvent::FuzzyToggle => {
                    self.as_mut_list_state().toggle_fuzzy();
                    self.update_search_status();
                }
                _ => {
                    self.as_mut_list_state().handle_search_input(key);
                    self.update_search_status();
                }
            }
            return;
        } else {
            match event {
                UserEvent::Quit => {
                    self.tx.send(AppEvent::Quit);
                }
                UserEvent::NavigateDown | UserEvent::SelectDown => {
                    for _ in 0..count {
                        self.as_mut_list_state().select_next();
                    }
                }
                UserEvent::NavigateUp | UserEvent::SelectUp => {
                    for _ in 0..count {
                        self.as_mut_list_state().select_prev();
                    }
                }
                UserEvent::GoToNextBranch | UserEvent::GoToPreviousBranch => {
                    self.as_mut_list_state()
                        .select_branch_tip(event == UserEvent::GoToNextBranch, count);
                }
                UserEvent::LineageRoot => {
                    if !self.as_mut_list_state().select_lineage_root() {
                        self.tx.send(AppEvent::UpdateStatusTransient(
                            "End of loaded first-parent ancestry".into(),
                        ));
                    }
                }
                UserEvent::LineageTip => {
                    self.open_lineage_picker();
                }
                UserEvent::GoToParent => {
                    for _ in 0..count {
                        self.as_mut_list_state().select_parent();
                    }
                }
                UserEvent::GoToTop => {
                    self.as_mut_list_state().select_first();
                }
                UserEvent::GoToBottom => {
                    self.as_mut_list_state().select_last();
                }
                UserEvent::ScrollDown => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_down();
                    }
                }
                UserEvent::ScrollUp => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_up();
                    }
                }
                UserEvent::PageDown => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_down_page();
                    }
                }
                UserEvent::PageUp => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_up_page();
                    }
                }
                UserEvent::HalfPageDown => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_down_half();
                    }
                }
                UserEvent::HalfPageUp => {
                    for _ in 0..count {
                        self.as_mut_list_state().scroll_up_half();
                    }
                }
                UserEvent::SelectTop => {
                    self.as_mut_list_state().select_high();
                }
                UserEvent::SelectMiddle => {
                    self.as_mut_list_state().select_middle();
                }
                UserEvent::SelectBottom => {
                    self.as_mut_list_state().select_low();
                }
                UserEvent::ShortCopy => {
                    self.copy_commit_short_hash();
                }
                UserEvent::FullCopy => {
                    self.copy_commit_hash();
                }
                UserEvent::CopyBranchName => {
                    self.tx.send(AppEvent::CopyBranchName(
                        self.as_list_state().selected_commit_hash().clone(),
                    ));
                }
                UserEvent::Search => {
                    self.as_mut_list_state().start_search();
                    self.update_search_status();
                }
                UserEvent::SearchTargetToggle => {
                    self.as_mut_list_state().toggle_search_target();
                    self.update_search_options_message();
                }
                UserEvent::IgnoreCaseToggle => {
                    self.as_mut_list_state().toggle_ignore_case();
                    self.update_search_options_message();
                }
                UserEvent::FuzzyToggle => {
                    self.as_mut_list_state().toggle_fuzzy();
                    self.update_search_options_message();
                }
                UserEvent::UserCommand(n) => {
                    self.tx.send(AppEvent::OpenUserCommand(n));
                }
                UserEvent::HelpToggle => {
                    self.tx.send(AppEvent::OpenHelp);
                }
                UserEvent::Cancel => {
                    self.as_mut_list_state().cancel_search();
                    self.clear_search_query();
                }
                UserEvent::Confirm => {
                    self.tx.send(AppEvent::OpenDetail);
                }
                UserEvent::RefList => {
                    self.tx.send(AppEvent::OpenRefs);
                }
                UserEvent::WorktreeList => {
                    self.tx.send(AppEvent::OpenWorktrees);
                }
                UserEvent::MarkToggle => {
                    self.as_mut_list_state().toggle_mark();
                }
                UserEvent::Refresh => {
                    self.refresh();
                }
                UserEvent::DeleteBranch | UserEvent::ForceDeleteBranch => {
                    self.tx.send(AppEvent::DeleteBranchAtCommit {
                        hash: self.as_list_state().selected_commit_hash().clone(),
                        force: event == UserEvent::ForceDeleteBranch,
                    });
                }
                UserEvent::SwitchBranch => {
                    self.tx.send(AppEvent::SwitchBranchAtSelectedCommit);
                }
                UserEvent::GraphNarrower => {
                    self.tx.send(AppEvent::ResizeGraph {
                        wider: false,
                        steps: count,
                    });
                }
                UserEvent::GraphWider => {
                    self.tx.send(AppEvent::ResizeGraph {
                        wider: true,
                        steps: count,
                    });
                }
                _ => {}
            }
        }

        if let SearchState::Applied { .. } = self.as_list_state().search_state() {
            match event {
                UserEvent::GoToNext => {
                    self.as_mut_list_state().select_next_match();
                    self.update_matched_message();
                }
                UserEvent::GoToPrevious => {
                    self.as_mut_list_state().select_prev_match();
                    self.update_matched_message();
                }
                _ => {}
            }
            // Do not return here
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let commit_list = CommitList::new(self.ctx.clone());
        f.render_stateful_widget(commit_list, area, self.as_mut_list_state());
    }

    pub fn update_layout(&mut self, area: Rect) {
        self.as_mut_list_state().update_height(area.height as usize);
    }

    pub fn prepare_graph_uploads(&mut self) {
        self.as_mut_list_state().ensure_visible_graph_uploaded();
    }
}

impl<'a> ListView<'a> {
    pub fn take_list_state(&mut self) -> CommitListState<'a> {
        self.commit_list_state.take().unwrap()
    }

    fn as_mut_list_state(&mut self) -> &mut CommitListState<'a> {
        self.commit_list_state.as_mut().unwrap()
    }

    pub fn as_list_state(&self) -> &CommitListState<'a> {
        self.commit_list_state.as_ref().unwrap()
    }

    pub fn drain_pending_graph_uploads(&mut self) -> Vec<String> {
        self.as_mut_list_state().drain_pending_graph_uploads()
    }

    pub fn graph_image_ids_sorted(&self) -> Vec<u32> {
        self.as_list_state().graph_image_ids_sorted()
    }

    fn open_lineage_picker(&mut self) {
        let (mut choices, selected) = self.as_list_state().lineage_choices();
        match choices.len() {
            0 => self.tx.send(AppEvent::UpdateStatusTransient(
                "No loaded first-parent descendants".into(),
            )),
            1 => {
                self.as_mut_list_state()
                    .select_lineage_tip(choices.remove(0).target);
            }
            _ => {
                self.lineage_picker = Some(LineagePicker { choices, selected });
                self.update_lineage_picker_status();
            }
        }
    }

    fn handle_lineage_picker(&mut self, event: UserEvent) {
        let mut picker = self.lineage_picker.take().expect("lineage picker is open");
        match event {
            UserEvent::Confirm => {
                self.as_mut_list_state()
                    .select_lineage_tip(picker.choices[picker.selected].target);
                self.tx.send(AppEvent::ClearStatusLine);
                return;
            }
            UserEvent::Cancel | UserEvent::Close | UserEvent::LineageRoot => {
                self.tx.send(AppEvent::ClearStatusLine);
                return;
            }
            UserEvent::Quit => {
                self.tx.send(AppEvent::Quit);
                return;
            }
            UserEvent::NavigateDown | UserEvent::SelectDown | UserEvent::LineageTip => {
                picker.selected = (picker.selected + 1) % picker.choices.len();
            }
            UserEvent::NavigateUp | UserEvent::SelectUp => {
                picker.selected =
                    (picker.selected + picker.choices.len() - 1) % picker.choices.len();
            }
            UserEvent::GoToTop => picker.selected = 0,
            UserEvent::GoToBottom => picker.selected = picker.choices.len() - 1,
            _ => {}
        }
        self.lineage_picker = Some(picker);
        self.update_lineage_picker_status();
    }

    fn update_lineage_picker_status(&self) {
        let picker = self
            .lineage_picker
            .as_ref()
            .expect("lineage picker is open");
        let message = format!(
            "Continue toward [{}/{}]: {}",
            picker.selected + 1,
            picker.choices.len(),
            picker.choices[picker.selected].label
        );
        self.tx.send(AppEvent::UpdateStatusInput {
            cursor_byte_index: message.len(),
            message,
            metadata: "j/k: choose · Enter: follow · Esc: cancel".into(),
        });
    }

    fn update_search_status(&self) {
        if let SearchState::Searching { .. } = self.as_list_state().search_state() {
            let list_state = self.as_list_state();
            if let Some(query) = list_state.search_query_string() {
                let cursor_byte_index = list_state.search_query_cursor_byte_index();
                let options = list_state.search_options().status_string();
                self.tx.send(AppEvent::UpdateStatusInput {
                    message: query,
                    cursor_byte_index,
                    metadata: options,
                });
            }
        }
    }

    fn update_search_options_message(&self) {
        if let SearchState::Applied { .. } = self.as_list_state().search_state() {
            self.update_matched_message();
        } else {
            let options = self.as_list_state().search_options().status_string();
            self.tx.send(AppEvent::UpdateStatusTransient(format!(
                "Search: {options}"
            )));
        }
    }

    fn clear_search_query(&self) {
        self.tx.send(AppEvent::ClearStatusLine);
    }

    fn update_matched_message(&self) {
        if let Some((msg, matched)) = self.as_list_state().matched_query_string() {
            let options = self.as_list_state().search_options().status_string();
            self.tx.send(AppEvent::UpdateSearchResult {
                message: msg,
                options,
                matched,
            });
        } else {
            self.tx.send(AppEvent::ClearStatusLine);
        }
    }

    fn copy_commit_short_hash(&self) {
        let selected = self.as_list_state().selected_commit_hash();
        self.copy_to_clipboard("Commit SHA (short)".into(), selected.as_short_hash().into());
    }

    fn copy_commit_hash(&self) {
        let selected = self.as_list_state().selected_commit_hash();
        self.copy_to_clipboard("Commit SHA".into(), selected.as_str().into());
    }

    fn copy_to_clipboard(&self, name: String, value: String) {
        self.tx.send(AppEvent::CopyToClipboard { name, value });
    }

    pub fn refresh_context(&self) -> RefreshViewContext {
        RefreshViewContext::List {
            list_context: ListRefreshViewContext::from(self.as_list_state()),
        }
    }

    pub fn refresh(&self) {
        self.tx.send(AppEvent::Refresh(self.refresh_context()));
    }

    pub fn reset_commit_list_with(&mut self, list_context: &ListRefreshViewContext) {
        let ListRefreshViewContext {
            commit_hash,
            marked_commit_hash,
            selected,
            height,
            scroll_to_top,
            index,
            search_options,
            search_context,
        } = list_context;
        let list_state = self.as_mut_list_state();
        list_state.restore_marked_commit_hash(marked_commit_hash.clone());
        list_state.restore_search_options(*search_options);
        list_state.reset_height(*height);
        if *scroll_to_top {
            list_state.select_first();
        } else {
            list_state.restore_commit_position(
                &CommitHash::from(commit_hash.as_str()),
                *index,
                *selected,
            );
        }
        if let Some(search_context) = search_context {
            list_state.restore_search(search_context);
        }
    }
}
