use std::{
    io::{self, Write},
    rc::Rc,
};

use ratatui::{
    backend::Backend,
    crossterm::{
        event::{KeyCode, KeyEvent},
        SynchronizedUpdate,
    },
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
    DefaultTerminal, Frame, Terminal,
};
use rustc_hash::FxHashMap;

use crate::{
    color::{ColorTheme, GraphColorSet},
    config::{CoreConfig, CursorType, UiConfig, UserCommand, UserCommandType},
    event::{AppEvent, EventController, UserEvent, UserEventWithCount},
    external::{
        copy_to_clipboard, exec_user_command, exec_user_command_suspend, ExternalCommandParameters,
    },
    git::{
        BranchTarget, BranchesAtCommit, Commit, CommitHash, DeleteBranchError, FileChange, Head,
        Ref, Repository,
    },
    graph::{GraphImageManager, GraphWidth},
    keybind::KeyBind,
    protocol::ImageProtocol,
    search::SearchOptions,
    view::{RefreshViewContext, View},
    widget::commit_list::{CommitInfo, CommitListState},
};

#[derive(Debug, Default)]
enum StatusLine {
    #[default]
    None,
    Input {
        message: String,
        cursor_byte_index: usize,
        metadata: String,
    },
    Transient(String),
    SearchResult {
        message: String,
        options: String,
        matched: bool,
    },
    NotificationInfo(String),
    NotificationSuccess(String),
    NotificationWarn(String),
    NotificationError(String),
}

#[derive(Clone, Copy)]
pub enum InitialSelection {
    Latest,
    Head,
}

pub enum Ret {
    Quit,
    Refresh(RefreshRequest),
}

pub struct RefreshRequest {
    pub context: RefreshViewContext,
    // shown as a success notification once the refreshed app is running
    pub message: Option<String>,
    // set when only the graph column width changes: the repository is not reloaded
    pub graph_width: Option<GraphWidth>,
}

const REMOTE_FORCE_UNSUPPORTED: &str =
    "Remote force deletion is not supported; use d for a separately confirmed remote deletion";

// A question on the status line; the next key press answers it.
#[derive(Debug)]
enum PendingPrompt {
    // y/N: switch although the worktree has uncommitted changes
    ConfirmSwitch(BranchTarget),
    // 1-9: which of the branches at the selected commit to switch to
    ChooseBranch(Vec<BranchTarget>),
    // 1-9: which branch name to copy, without changing repository state
    ChooseBranchNameToCopy(Vec<String>),
    ChooseBranchToDelete {
        targets: Vec<BranchTarget>,
        force: bool,
        offset: usize,
    },
    ConfirmDelete {
        target: BranchTarget,
        force: bool,
    },
}

#[derive(Debug)]
pub struct AppContext {
    pub keybind: KeyBind,
    pub core_config: CoreConfig,
    pub ui_config: UiConfig,
    pub color_theme: ColorTheme,
    pub image_protocol: ImageProtocol,
}

#[derive(Debug, Default)]
enum PendingClear {
    #[default]
    None,
    ViewImages,
    Screen,
}

#[derive(Debug, Default)]
struct AppStatus {
    status_line: StatusLine,
    numeric_prefix: String,
    view_area: Rect,
    pending_clear: PendingClear,
    pending_prompt: Option<PendingPrompt>,
    pending_refresh_message: Option<String>,
    pending_graph_width: Option<GraphWidth>,
}

#[derive(Debug)]
pub struct App<'a> {
    repository: &'a Repository,
    view: View<'a>,
    app_status: AppStatus,
    graph_width: GraphWidth,
    graph_lane_count: usize,
    ctx: Rc<AppContext>,
    ec: &'a EventController,
}

impl<'a> App<'a> {
    pub fn new(
        repository: &'a Repository,
        graph_image_manager: GraphImageManager<'a>,
        graph_color_set: &'a GraphColorSet,
        initial_selection: InitialSelection,
        ctx: Rc<AppContext>,
        ec: &'a EventController,
    ) -> Self {
        let graph = graph_image_manager.graph();
        let mut ref_name_to_commit_index_map = FxHashMap::default();
        let commits = graph
            .commits
            .iter()
            .enumerate()
            .map(|(i, commit)| {
                let refs = repository.refs(&commit.commit_hash);
                for r in &refs {
                    ref_name_to_commit_index_map.insert(r.name(), i);
                }
                let (pos_x, _) = graph.commit_pos_map[&commit.commit_hash];
                let graph_color = graph_color_set.get(pos_x).to_ratatui_color();
                CommitInfo::new(commit, refs, graph_color)
            })
            .collect();
        let graph_cell_width = graph_image_manager.graph_cell_width();
        let graph_width = graph_image_manager.graph_width();
        let graph_lane_count = graph_image_manager.lane_count();
        let head = repository.head();
        let mut commit_list_state = CommitListState::new(
            commits,
            graph_image_manager,
            graph_cell_width,
            head,
            ref_name_to_commit_index_map,
            SearchOptions {
                target: ctx.core_config.search.target,
                ignore_case: ctx.core_config.search.ignore_case,
                fuzzy: ctx.core_config.search.fuzzy,
            },
            ctx.ui_config.list.scrolloff as usize,
        );
        if let InitialSelection::Head = initial_selection {
            match repository.head() {
                Head::Branch { name } => commit_list_state.select_ref(name),
                Head::Detached { target } => commit_list_state.select_commit_hash(target),
                Head::None => {}
            }
        }
        let view = View::of_list(commit_list_state, ctx.clone(), ec.sender());

        let mut app = Self {
            repository,
            view,
            app_status: AppStatus::default(),
            graph_width,
            graph_lane_count,
            ctx,
            ec,
        };

        if let Some(warning) = &graph.warning {
            app.warn_notification(warning.clone());
        }

        app
    }
}

impl App<'_> {
    pub fn run(
        &mut self,
        terminal: &mut DefaultTerminal,
        refresh_view_context: Option<RefreshViewContext>,
    ) -> Result<Ret, std::io::Error> {
        if let Some(context) = refresh_view_context {
            let [view_area, _] = split_app_areas(terminal.size()?.into());
            self.update_state(view_area);
            self.init_with_context(context);
        }

        self.app_status.pending_clear = PendingClear::Screen;

        loop {
            self.prepare_render(terminal)?;
            // Keep image deletion, uploads, and text updates in the same frame.
            // Preparation and event handling stay outside the synchronized update.
            io::stdout().sync_update(|_| self.draw(terminal))??;
            match self.ec.recv() {
                AppEvent::Key(key) => {
                    if let Some(pending) = self.app_status.pending_prompt.take() {
                        self.clear_status_line();
                        if let Some(UserEvent::ForceQuit) = self.ctx.keybind.get(&key) {
                            self.ec.send(AppEvent::Quit);
                        } else {
                            self.answer_prompt(pending, key);
                        }
                        // The key answered the prompt and is not passed on.
                        continue;
                    }

                    match self.app_status.status_line {
                        StatusLine::None | StatusLine::Input { .. } => {
                            // do nothing
                        }
                        StatusLine::Transient(_)
                        | StatusLine::SearchResult { .. }
                        | StatusLine::NotificationInfo(_)
                        | StatusLine::NotificationSuccess(_)
                        | StatusLine::NotificationWarn(_) => {
                            // Clear message and pass key input as is
                            self.clear_status_line();
                        }
                        StatusLine::NotificationError(_) => {
                            // Clear message and cancel key input
                            self.clear_status_line();
                            continue;
                        }
                    }

                    let user_event = self.ctx.keybind.get(&key);

                    if let Some(UserEvent::Cancel) = user_event {
                        if !self.app_status.numeric_prefix.is_empty() {
                            // Clear numeric prefix and cancel the event
                            self.app_status.numeric_prefix.clear();
                            continue;
                        }
                    }

                    match user_event {
                        Some(UserEvent::ForceQuit) => {
                            self.ec.send(AppEvent::Quit);
                        }
                        Some(ue) => {
                            let event_with_count =
                                process_numeric_prefix(&self.app_status.numeric_prefix, *ue, key);
                            self.view.handle_event(event_with_count, key);
                            self.app_status.numeric_prefix.clear();
                        }
                        None => {
                            if let StatusLine::Input { .. } = self.app_status.status_line {
                                // In input mode, pass all key events to the view
                                // fixme: currently, the only thing that processes key_event is searching the list,
                                //        so this probably works, but it's not the right process...
                                self.app_status.numeric_prefix.clear();
                                self.view.handle_event(
                                    UserEventWithCount::from_event(UserEvent::Unknown),
                                    key,
                                );
                            } else if let KeyCode::Char(c) = key.code {
                                // Accumulate numeric prefix
                                if c.is_ascii_digit()
                                    && (c != '0' || !self.app_status.numeric_prefix.is_empty())
                                {
                                    self.app_status.numeric_prefix.push(c);
                                }
                            }
                        }
                    }
                }
                AppEvent::Resize(w, h) => {
                    let _ = (w, h);
                }
                AppEvent::Quit => {
                    self.cleanup_graph_images()?;
                    return Ok(Ret::Quit);
                }
                AppEvent::OpenDetail => {
                    self.open_detail();
                    self.app_status.pending_clear = PendingClear::ViewImages;
                }
                AppEvent::CloseDetail => {
                    self.close_detail();
                    self.app_status.pending_clear = PendingClear::Screen;
                }
                AppEvent::OpenUserCommand(n) => {
                    self.open_user_command(n);
                }
                AppEvent::CloseUserCommand => {
                    self.close_user_command();
                    self.app_status.pending_clear = PendingClear::Screen;
                }
                AppEvent::OpenRefs => {
                    self.open_refs();
                }
                AppEvent::CloseRefs => {
                    self.close_refs();
                }
                AppEvent::OpenHelp => {
                    self.open_help();
                    self.app_status.pending_clear = PendingClear::ViewImages;
                }
                AppEvent::CloseHelp => {
                    self.close_help();
                    self.app_status.pending_clear = PendingClear::Screen;
                }
                AppEvent::SelectOlderCommit => {
                    self.select_older_commit();
                }
                AppEvent::SelectNewerCommit => {
                    self.select_newer_commit();
                }
                AppEvent::SelectParentCommit => {
                    self.select_parent_commit();
                }
                AppEvent::SwitchBranch(target) => {
                    self.request_switch_branch(target);
                }
                AppEvent::SwitchBranchAtSelectedCommit => {
                    self.switch_branch_at_selected_commit();
                }
                AppEvent::DeleteBranch { target, force } => {
                    self.request_delete_branch(target, force);
                }
                AppEvent::DeleteBranchAtCommit { hash, force } => {
                    self.delete_branch_at_commit(&hash, force);
                }
                AppEvent::CopyBranchName(hash) => {
                    self.copy_branch_name(&hash);
                }
                AppEvent::ResizeGraph { wider, steps } => {
                    self.resize_graph(wider, steps);
                }
                AppEvent::CopyToClipboard { name, value } => {
                    self.copy_to_clipboard(name, value);
                }
                AppEvent::Refresh(context) => {
                    self.cleanup_graph_images()?;
                    let message = self.app_status.pending_refresh_message.take();
                    let graph_width = self.app_status.pending_graph_width.take();
                    let request = RefreshRequest {
                        context,
                        message,
                        graph_width,
                    };
                    return Ok(Ret::Refresh(request));
                }
                AppEvent::ClearStatusLine => {
                    self.clear_status_line();
                }
                AppEvent::UpdateStatusInput {
                    message,
                    cursor_byte_index,
                    metadata,
                } => {
                    self.update_status_input(message, cursor_byte_index, metadata);
                }
                AppEvent::UpdateStatusTransient(msg) => {
                    self.update_status_transient(msg);
                }
                AppEvent::UpdateSearchResult {
                    message,
                    options,
                    matched,
                } => {
                    self.update_search_result(message, options, matched);
                }
                AppEvent::NotifyInfo(msg) => {
                    self.info_notification(msg);
                }
                AppEvent::NotifySuccess(msg) => {
                    self.success_notification(msg);
                }
                AppEvent::NotifyWarn(msg) => {
                    self.warn_notification(msg);
                }
                AppEvent::NotifyError(msg) => {
                    self.error_notification(msg);
                }
            }
        }
    }

    fn prepare_render(&mut self, terminal: &mut DefaultTerminal) -> Result<(), std::io::Error> {
        let area: Rect = terminal.size()?.into();
        let [view_area, _] = split_app_areas(area);
        self.update_state(view_area);
        self.view.update_layout(view_area);
        self.view.prepare_graph_uploads();
        Ok(())
    }

    fn draw(&mut self, terminal: &mut DefaultTerminal) -> Result<(), std::io::Error> {
        match self.app_status.pending_clear {
            PendingClear::None => {}
            PendingClear::ViewImages => self.clear_view_image_area(terminal)?,
            PendingClear::Screen => {
                self.ctx.image_protocol.clear();
                clear_fullscreen(terminal)?;
            }
        }
        self.app_status.pending_clear = PendingClear::None;
        self.flush_pending_graph_uploads()?;
        terminal.draw(|f| self.render(f))?;
        Ok(())
    }

    fn flush_pending_graph_uploads(&mut self) -> Result<(), std::io::Error> {
        let uploads = self.view.drain_pending_graph_uploads();
        if uploads.is_empty() {
            return Ok(());
        }

        let mut stdout = io::stdout().lock();
        for upload in uploads {
            stdout.write_all(upload.as_bytes())?;
        }
        stdout.flush()
    }

    fn cleanup_graph_images(&self) -> Result<(), std::io::Error> {
        let image_ids = self.view.graph_image_ids_sorted();
        self.ctx.image_protocol.delete_images(&image_ids)
    }

    fn render(&mut self, f: &mut Frame) {
        let base = Block::default()
            .fg(self.ctx.color_theme.fg)
            .bg(self.ctx.color_theme.bg);
        f.render_widget(base, f.area());

        let [view_area, status_line_area] = split_app_areas(f.area());

        self.update_state(view_area);

        self.view.render(f, view_area);
        self.render_status_line(f, status_line_area);
    }
}

impl App<'_> {
    fn render_status_line(&self, f: &mut Frame, area: Rect) {
        let text: Line = match &self.app_status.status_line {
            StatusLine::None => {
                if self.app_status.numeric_prefix.is_empty() {
                    Line::raw("")
                } else {
                    Line::raw(self.app_status.numeric_prefix.as_str())
                        .fg(self.ctx.color_theme.status_input_transient_fg)
                }
            }
            StatusLine::Input {
                message,
                cursor_byte_index,
                metadata,
            } => status_line_with_metadata(
                status_input_line(
                    message,
                    *cursor_byte_index,
                    &self.ctx.ui_config.common.cursor_type,
                    Style::default().fg(self.ctx.color_theme.status_input_fg),
                    Style::default().fg(self.ctx.color_theme.virtual_cursor_fg),
                ),
                metadata,
                Style::default().fg(self.ctx.color_theme.status_input_transient_fg),
                area.width,
            ),
            StatusLine::Transient(msg) => {
                let msg_w = console::measure_text_width(msg.as_str());
                let pad_w = (area.width as usize).saturating_sub(msg_w + 2 /* pad */);
                Line::from(vec![
                    " ".repeat(pad_w).into(),
                    msg.as_str()
                        .fg(self.ctx.color_theme.status_input_transient_fg),
                ])
            }
            StatusLine::SearchResult {
                message,
                options,
                matched,
            } => {
                let message_style = if *matched {
                    Style::default().fg(self.ctx.color_theme.status_info_fg)
                } else {
                    Style::default()
                        .fg(self.ctx.color_theme.status_warn_fg)
                        .add_modifier(Modifier::BOLD)
                };
                status_line_with_metadata(
                    Line::from(Span::styled(message.as_str(), message_style)),
                    options,
                    Style::default().fg(self.ctx.color_theme.status_input_transient_fg),
                    area.width,
                )
            }
            StatusLine::NotificationInfo(msg) => {
                Line::raw(msg).fg(self.ctx.color_theme.status_info_fg)
            }
            StatusLine::NotificationSuccess(msg) => Line::raw(msg)
                .add_modifier(Modifier::BOLD)
                .fg(self.ctx.color_theme.status_success_fg),
            StatusLine::NotificationWarn(msg) => Line::raw(msg)
                .add_modifier(Modifier::BOLD)
                .fg(self.ctx.color_theme.status_warn_fg),
            StatusLine::NotificationError(msg) => Line::raw(format!("ERROR: {msg}"))
                .add_modifier(Modifier::BOLD)
                .fg(self.ctx.color_theme.status_error_fg),
        };
        let paragraph = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::TOP)
                .style(Style::default().fg(self.ctx.color_theme.divider_fg))
                .padding(Padding::horizontal(1)),
        );
        f.render_widget(paragraph, area);

        if let (
            StatusLine::Input {
                message,
                cursor_byte_index,
                ..
            },
            CursorType::Native,
        ) = (
            &self.app_status.status_line,
            &self.ctx.ui_config.common.cursor_type,
        ) {
            let cursor_position = Line::raw(&message[..*cursor_byte_index]).width() as u16;
            let (x, y) = (area.x + cursor_position + 1, area.y + 1);
            f.set_cursor_position((x, y));
        }
    }
}

fn split_app_areas(area: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(area)
}

fn clear_fullscreen<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), B::Error> {
    // Terminal::clear queries the cursor position, competing with our input thread.
    // A fullscreen redraw positions the cursor itself, so only clear the screen and
    // reset both diff buffers. Swapping twice preserves the current buffer index.
    terminal.backend_mut().clear()?;
    terminal.swap_buffers();
    terminal.swap_buffers();
    Ok(())
}

impl App<'_> {
    fn update_state(&mut self, view_area: Rect) {
        self.app_status.view_area = view_area;
    }

    fn clear_view_image_area(&self, terminal: &mut DefaultTerminal) -> Result<(), std::io::Error> {
        if let Some(area) = self.view.image_clear_area(self.app_status.view_area) {
            let area = area.intersection(terminal.size()?.into());
            for y in area.top()..area.bottom() {
                self.ctx.image_protocol.clear_line(y);
            }
        }
        Ok(())
    }

    fn open_detail(&mut self) {
        let commit_list_state = match self.view {
            View::List(ref mut view) => view.take_list_state(),
            View::UserCommand(ref mut view) => view.take_list_state(),
            _ => return,
        };
        let (commit, changes, refs) = selected_commit_details(self.repository, &commit_list_state);
        self.view = View::of_detail(
            commit_list_state,
            commit,
            changes,
            refs,
            self.ctx.clone(),
            self.ec.sender(),
        );
    }

    fn close_detail(&mut self) {
        if let View::Detail(ref mut view) = self.view {
            let commit_list_state = view.take_list_state();
            self.view = View::of_list(commit_list_state, self.ctx.clone(), self.ec.sender());
        }
    }

    fn open_user_command(&mut self, user_command_number: usize) {
        match extract_user_command_by_number(user_command_number, &self.ctx).map(|c| &c.r#type) {
            Ok(UserCommandType::Inline) => {
                self.open_user_command_inline(user_command_number);
                self.app_status.pending_clear = PendingClear::ViewImages;
            }
            Ok(UserCommandType::Silent) => {
                self.open_user_command_silent(user_command_number);
                self.app_status.pending_clear = PendingClear::Screen;
            }
            Ok(UserCommandType::Suspend) => {
                self.open_user_command_suspend(user_command_number);
                self.app_status.pending_clear = PendingClear::Screen;
            }
            Err(err) => {
                self.ec.send(AppEvent::NotifyError(err));
            }
        };
    }

    fn open_user_command_inline(&mut self, user_command_number: usize) {
        let commit_list_state = match self.view {
            View::List(ref mut view) => view.as_list_state(),
            View::Detail(ref mut view) => view.as_list_state(),
            View::UserCommand(ref mut view) => view.as_list_state(),
            _ => return,
        };
        let (commit, _, refs) = selected_commit_details(self.repository, commit_list_state);
        let result = build_external_command_parameters_and_exec_command(
            &commit,
            &refs,
            commit_list_state.marked_commit_hash(),
            user_command_number,
            self.app_status.view_area,
            &self.ctx,
        );
        match result {
            Ok(output) => {
                // take list state only when the command execution is successful, to avoid losing the state when the command fails
                let commit_list_state = match self.view {
                    View::List(ref mut view) => view.take_list_state(),
                    View::Detail(ref mut view) => view.take_list_state(),
                    View::UserCommand(ref mut view) => view.take_list_state(),
                    _ => return,
                };
                self.view = View::of_user_command(
                    commit_list_state,
                    output,
                    user_command_number,
                    self.ctx.clone(),
                    self.ec.sender(),
                );
            }
            Err(err) => {
                self.ec.send(AppEvent::NotifyError(err));
            }
        };
    }

    fn open_user_command_silent(&mut self, user_command_number: usize) {
        let commit_list_state = match self.view {
            View::List(ref mut view) => view.as_list_state(),
            View::Detail(ref mut view) => view.as_list_state(),
            View::UserCommand(ref mut view) => view.as_list_state(),
            _ => return,
        };
        let (commit, _, refs) = selected_commit_details(self.repository, commit_list_state);
        let result = build_external_command_parameters_and_exec_command(
            &commit,
            &refs,
            commit_list_state.marked_commit_hash(),
            user_command_number,
            self.app_status.view_area,
            &self.ctx,
        );
        match result {
            Ok(_) => {
                if extract_user_command_refresh_by_number(user_command_number, &self.ctx) {
                    self.view.refresh();
                }
            }
            Err(err) => {
                self.ec.send(AppEvent::NotifyError(err));
            }
        }
    }

    fn open_user_command_suspend(&mut self, user_command_number: usize) {
        let commit_list_state = match self.view {
            View::List(ref mut view) => view.as_list_state(),
            View::Detail(ref mut view) => view.as_list_state(),
            View::UserCommand(ref mut view) => view.as_list_state(),
            _ => return,
        };
        let (commit, _, refs) = selected_commit_details(self.repository, commit_list_state);
        match build_external_command_parameters(
            &commit,
            &refs,
            commit_list_state.marked_commit_hash(),
            user_command_number,
            self.app_status.view_area,
            &self.ctx,
        ) {
            Ok(params) => {
                self.ec.suspend();
                let exec_result = exec_user_command_suspend(params);
                self.ec.resume();

                if extract_user_command_refresh_by_number(user_command_number, &self.ctx) {
                    self.view.refresh();
                }

                // notify after resuming and refreshing
                if let Err(err) = exec_result {
                    self.ec.send(AppEvent::NotifyError(err));
                }
            }
            Err(err) => {
                self.ec.send(AppEvent::NotifyError(err));
            }
        }
    }

    fn close_user_command(&mut self) {
        if let View::UserCommand(ref mut view) = self.view {
            let commit_list_state = view.take_list_state();
            self.view = View::of_list(commit_list_state, self.ctx.clone(), self.ec.sender());
        }
    }

    fn open_refs(&mut self) {
        if let View::List(ref mut view) = self.view {
            let commit_list_state = view.take_list_state();
            let refs = self.repository.all_refs();
            let current_branch = match self.repository.head() {
                Head::Branch { name } => Some(name.clone()),
                Head::Detached { .. } | Head::None => None,
            };
            let other_worktree_branches = self
                .repository
                .other_worktree_branches()
                .keys()
                .cloned()
                .collect();
            self.view = View::of_refs(
                commit_list_state,
                &refs,
                current_branch,
                other_worktree_branches,
                self.ctx.clone(),
                self.ec.sender(),
            );
        }
    }

    fn close_refs(&mut self) {
        if let View::Refs(ref mut view) = self.view {
            let commit_list_state = view.take_list_state();
            self.view = View::of_list(commit_list_state, self.ctx.clone(), self.ec.sender());
        }
    }

    fn open_help(&mut self) {
        let before_view = std::mem::take(&mut self.view);
        self.view = View::of_help(before_view, self.ctx.clone(), self.ec.sender());
    }

    fn close_help(&mut self) {
        if let View::Help(ref mut view) = self.view {
            self.view = view.take_before_view();
        }
    }

    fn select_older_commit(&mut self) {
        if let View::Detail(ref mut view) = self.view {
            view.select_older_commit(self.repository);
        } else if let View::UserCommand(ref mut view) = self.view {
            view.select_older_commit(
                self.repository,
                self.app_status.view_area,
                build_external_command_parameters_and_exec_command,
            );
        }
    }

    fn select_newer_commit(&mut self) {
        if let View::Detail(ref mut view) = self.view {
            view.select_newer_commit(self.repository);
        } else if let View::UserCommand(ref mut view) = self.view {
            view.select_newer_commit(
                self.repository,
                self.app_status.view_area,
                build_external_command_parameters_and_exec_command,
            );
        }
    }

    fn select_parent_commit(&mut self) {
        if let View::Detail(ref mut view) = self.view {
            view.select_parent_commit(self.repository);
        } else if let View::UserCommand(ref mut view) = self.view {
            view.select_parent_commit(
                self.repository,
                self.app_status.view_area,
                build_external_command_parameters_and_exec_command,
            );
        }
    }

    fn init_with_context(&mut self, context: RefreshViewContext) {
        if let View::List(ref mut view) = self.view {
            view.reset_commit_list_with(context.list_context());
        }
        match context {
            RefreshViewContext::List { .. } => {}
            RefreshViewContext::Detail { .. } => {
                self.open_detail();
            }
            RefreshViewContext::UserCommand {
                user_command_context,
                ..
            } => {
                self.open_user_command(user_command_context.n);
            }
            RefreshViewContext::Refs { refs_context, .. } => {
                self.open_refs();
                if let View::Refs(ref mut view) = self.view {
                    view.reset_refs_with(refs_context);
                }
            }
        }
    }

    fn clear_status_line(&mut self) {
        self.app_status.status_line = StatusLine::None;
    }

    fn update_status_input(&mut self, message: String, cursor_byte_index: usize, metadata: String) {
        self.app_status.status_line = StatusLine::Input {
            message,
            cursor_byte_index,
            metadata,
        };
    }

    fn update_status_transient(&mut self, msg: String) {
        self.app_status.status_line = StatusLine::Transient(msg);
    }

    fn update_search_result(&mut self, message: String, options: String, matched: bool) {
        self.app_status.status_line = StatusLine::SearchResult {
            message,
            options,
            matched,
        };
    }

    fn info_notification(&mut self, msg: String) {
        self.app_status.status_line = StatusLine::NotificationInfo(msg);
    }

    pub fn success_notification(&mut self, msg: String) {
        self.app_status.status_line = StatusLine::NotificationSuccess(msg);
    }

    pub fn warn_notification(&mut self, msg: String) {
        self.app_status.status_line = StatusLine::NotificationWarn(msg);
    }

    fn error_notification(&mut self, msg: String) {
        self.app_status.status_line = StatusLine::NotificationError(msg);
    }

    fn request_switch_branch(&mut self, target: BranchTarget) {
        if let BranchTarget::Local(name) = &target {
            if matches!(self.repository.head(), Head::Branch { name: current } if current == name) {
                self.info_notification(format!("Already on '{name}'"));
                return;
            }
            // Never take a branch away from the worktree, and possibly the agent, using it.
            if let Some(path) = self.repository.other_worktree_branches().get(name) {
                self.error_notification(format!(
                    "'{name}' is checked out in worktree {}; not switching",
                    path.display()
                ));
                return;
            }
        }
        match self.repository.has_uncommitted_changes() {
            Ok(true) => {
                self.warn_notification(format!(
                    "This worktree has uncommitted changes. Switch to '{}' anyway? [y/N]",
                    target.name()
                ));
                self.app_status.pending_prompt = Some(PendingPrompt::ConfirmSwitch(target));
            }
            Ok(false) => self.switch_branch(target),
            Err(err) => self.error_notification(err),
        }
    }

    // Narrows or widens the graph column by redrawing the graph at another width.
    fn resize_graph(&mut self, wider: bool, steps: usize) {
        let lane_count = self.graph_lane_count;
        let term_width = self.app_status.view_area.width as usize;
        let mut width = self.graph_width;
        for _ in 0..steps {
            let next = if wider {
                width.wider(lane_count, term_width)
            } else {
                width.narrower(lane_count)
            };
            match next {
                Some(next) => width = next,
                None => break,
            }
        }
        if width == self.graph_width {
            let message = if !wider {
                "The graph is at its narrowest"
            } else if width.lanes(lane_count) == lane_count {
                "The graph is at its widest"
            } else {
                "No room for a wider graph"
            };
            self.info_notification(message.into());
            return;
        }
        self.app_status.pending_graph_width = Some(width);
        self.app_status.pending_refresh_message = Some(width.describe(lane_count));
        self.view.refresh();
    }

    // Space on a commit switches to the branch whose head it is.
    fn switch_branch_at_selected_commit(&mut self) {
        let View::List(view) = &self.view else {
            return;
        };
        let selected = view.as_list_state().selected_commit_hash().clone();
        let BranchesAtCommit {
            mut targets,
            has_current,
            shadowed_remotes,
        } = self.repository.branches_at(&selected);

        match targets.len() {
            0 => {
                if let (true, Head::Branch { name }) = (has_current, self.repository.head()) {
                    self.info_notification(format!("Already on '{name}'"));
                } else if let Some((remote, local)) = shadowed_remotes.first() {
                    self.warn_notification(format!(
                        "'{remote}' already has a local branch '{local}' at another commit; switch from there"
                    ));
                } else {
                    self.info_notification("No branch at this commit".into());
                }
            }
            1 => self.request_switch_branch(targets.remove(0)),
            _ => {
                // The answer is a single digit.
                targets.truncate(9);
                let choices: Vec<String> = targets
                    .iter()
                    .enumerate()
                    .map(|(i, target)| format!("[{}] {}", i + 1, target.name()))
                    .collect();
                self.info_notification(format!("Switch to: {}", choices.join("  ")));
                self.app_status.pending_prompt = Some(PendingPrompt::ChooseBranch(targets));
            }
        }
    }

    fn copy_branch_name(&mut self, hash: &CommitHash) {
        let mut names = self.repository.branch_names_for_copy(hash);
        match names.len() {
            0 => self.info_notification("No branch at this commit; nothing copied".into()),
            1 => self.copy_to_clipboard("Branch Name".into(), names.remove(0)),
            _ => {
                names.truncate(9);
                let choices: Vec<String> = names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| format!("[{}] {name}", i + 1))
                    .collect();
                self.info_notification(format!("Copy branch name: {}", choices.join("  ")));
                self.app_status.pending_prompt = Some(PendingPrompt::ChooseBranchNameToCopy(names));
            }
        }
    }

    fn choose_branch_to_delete(&mut self, targets: Vec<BranchTarget>, force: bool, offset: usize) {
        let choices: Vec<String> = targets
            .iter()
            .skip(offset)
            .take(9)
            .enumerate()
            .map(|(i, target)| {
                let kind = match target {
                    BranchTarget::Local(_) => "local",
                    BranchTarget::Remote(_) => "REMOTE",
                };
                format!("[{}] {kind} {}", i + 1, target.name())
            })
            .collect();
        let more = if targets.len() > 9 {
            "  [n] next page"
        } else {
            ""
        };
        self.info_notification(format!("Delete which branch? {}{more}", choices.join("  ")));
        self.app_status.pending_prompt = Some(PendingPrompt::ChooseBranchToDelete {
            targets,
            force,
            offset,
        });
    }

    fn delete_branch_at_commit(&mut self, hash: &CommitHash, force: bool) {
        let mut targets = self.repository.branches_for_deletion(hash);
        if targets.is_empty() {
            self.info_notification("No branch at this commit; nothing deleted".into());
            return;
        }
        // Force deletion is local-only, so remote choices would only be refused.
        if force {
            targets.retain(|target| matches!(target, BranchTarget::Local(_)));
            if targets.is_empty() {
                self.warn_notification(REMOTE_FORCE_UNSUPPORTED.into());
                return;
            }
        }
        match targets.len() {
            1 => self.request_delete_branch(targets.remove(0), force),
            _ => self.choose_branch_to_delete(targets, force, 0),
        }
    }

    fn request_delete_branch(&mut self, target: BranchTarget, force: bool) {
        if force && matches!(target, BranchTarget::Remote(_)) {
            self.warn_notification(REMOTE_FORCE_UNSUPPORTED.into());
            return;
        }
        if let Err(error) = self.repository.check_branch_deletion(&target) {
            self.error_notification(error);
            return;
        }
        let question = match &target {
            BranchTarget::Local(name) if force => format!("FORCE DELETE local branch '{name}', even if unmerged? [y/N]"),
            BranchTarget::Local(name) => format!("Delete local branch '{name}'? Remote branch is unchanged. [y/N]"),
            BranchTarget::Remote(name) => format!("Delete REMOTE branch '{name}' FROM THE SERVER? Local branches are unchanged. [y/N]"),
        };
        self.warn_notification(question);
        self.app_status.pending_prompt = Some(PendingPrompt::ConfirmDelete { target, force });
    }

    fn delete_branch(&mut self, target: BranchTarget, force: bool) {
        match self.repository.delete_branch(&target, force) {
            Ok(()) => {
                self.app_status.pending_refresh_message = Some(format!(
                    "Deleted {} branch '{}'",
                    match target {
                        BranchTarget::Local(_) => "local",
                        BranchTarget::Remote(_) => "remote",
                    },
                    target.name()
                ));
                self.view.refresh();
            }
            Err(DeleteBranchError::NotMerged(error)) => {
                self.warn_notification(format!(
                    "{error} — use force-delete (D by default) for a separate confirmation"
                ));
            }
            Err(DeleteBranchError::Other(error)) => self.error_notification(error),
        }
    }

    // Any key other than the expected answer cancels the prompt.
    fn answer_prompt(&mut self, pending: PendingPrompt, key: KeyEvent) {
        match (pending, key.code) {
            (PendingPrompt::ConfirmDelete { target, force }, KeyCode::Char('y' | 'Y')) => {
                self.delete_branch(target, force);
            }
            (
                PendingPrompt::ChooseBranchToDelete {
                    mut targets,
                    force,
                    offset,
                },
                KeyCode::Char(c),
            ) => {
                if c == 'n' && targets.len() > 9 {
                    let next = if offset + 9 < targets.len() {
                        offset + 9
                    } else {
                        0
                    };
                    self.choose_branch_to_delete(targets, force, next);
                } else if let Some(index) = c
                    .to_digit(10)
                    .filter(|d| (1..=9).contains(d))
                    .map(|d| offset + d as usize - 1)
                    .filter(|i| *i < targets.len())
                {
                    self.request_delete_branch(targets.swap_remove(index), force);
                }
            }
            (PendingPrompt::ConfirmSwitch(target), KeyCode::Char('y' | 'Y')) => {
                self.switch_branch(target);
            }
            (PendingPrompt::ChooseBranchNameToCopy(mut names), KeyCode::Char(c)) => {
                let index = c.to_digit(10).and_then(|d| (d as usize).checked_sub(1));
                if let Some(index) = index.filter(|i| *i < names.len()) {
                    self.copy_to_clipboard("Branch Name".into(), names.swap_remove(index));
                }
            }
            (PendingPrompt::ChooseBranch(mut targets), KeyCode::Char(c)) => {
                let index = c.to_digit(10).and_then(|d| (d as usize).checked_sub(1));
                if let Some(index) = index.filter(|i| *i < targets.len()) {
                    self.request_switch_branch(targets.swap_remove(index));
                }
            }
            _ => {}
        }
    }

    fn switch_branch(&mut self, target: BranchTarget) {
        match self.repository.switch_branch(&target) {
            Ok(()) => {
                let message = match &target {
                    BranchTarget::Local(name) => format!("Switched to branch '{name}'"),
                    BranchTarget::Remote(name) => {
                        format!("Switched to a new branch tracking '{name}'")
                    }
                };
                // The graph, HEAD and branch markers are reloaded; the message is shown afterwards.
                self.app_status.pending_refresh_message = Some(message);
                self.view.refresh();
            }
            Err(err) => self.error_notification(err),
        }
    }

    fn copy_to_clipboard(&self, name: String, value: String) {
        match copy_to_clipboard(value, &self.ctx.core_config.external.clipboard) {
            Ok(_) => {
                let msg = format!("Copied {name} to clipboard successfully");
                self.ec.send(AppEvent::NotifySuccess(msg));
            }
            Err(msg) => {
                self.ec.send(AppEvent::NotifyError(msg));
            }
        }
    }
}

fn status_input_line<'a>(
    message: &'a str,
    cursor_byte_index: usize,
    cursor_type: &'a CursorType,
    input_style: Style,
    cursor_style: Style,
) -> Line<'a> {
    match cursor_type {
        CursorType::Native => Line::from(Span::styled(message, input_style)),
        CursorType::Virtual(cursor) => {
            let (before, after) = message.split_at(cursor_byte_index);
            Line::from(vec![
                Span::styled(before, input_style),
                Span::styled(cursor.as_str(), cursor_style),
                Span::styled(after, input_style),
            ])
        }
    }
}

fn status_line_with_metadata<'a>(
    mut message: Line<'a>,
    metadata: &'a str,
    metadata_style: Style,
    area_width: u16,
) -> Line<'a> {
    let content_width = area_width.saturating_sub(2) as usize;
    let message_width = message.width();
    let metadata_width = Span::raw(metadata).width();
    let min_gap_width = 2;

    if message_width + min_gap_width + metadata_width > content_width {
        return message;
    }

    let pad_width = content_width - message_width - metadata_width;
    message.spans.extend([
        Span::raw(" ".repeat(pad_width)),
        Span::styled(metadata, metadata_style),
    ]);
    message
}

fn selected_commit_details(
    repository: &Repository,
    commit_list_state: &CommitListState,
) -> (Commit, Vec<FileChange>, Vec<Ref>) {
    let selected = commit_list_state.selected_commit_hash().clone();
    let (commit, changes) = repository.commit_detail(&selected);
    let refs: Vec<Ref> = repository.refs(&selected).into_iter().cloned().collect();
    (commit, changes, refs)
}

fn process_numeric_prefix(
    numeric_prefix: &str,
    user_event: UserEvent,
    _key_event: KeyEvent,
) -> UserEventWithCount {
    if user_event.is_countable() {
        let count = if numeric_prefix.is_empty() {
            1
        } else {
            numeric_prefix.parse::<usize>().unwrap_or(1)
        };
        UserEventWithCount::new(user_event, count)
    } else {
        UserEventWithCount::from_event(user_event)
    }
}

fn extract_user_command_by_number(
    user_command_number: usize,
    ctx: &AppContext,
) -> Result<&UserCommand, String> {
    ctx.core_config
        .user_command
        .commands
        .get(&user_command_number.to_string())
        .ok_or_else(|| format!("No user command configured for number {user_command_number}",))
}

fn extract_user_command_refresh_by_number(user_command_number: usize, ctx: &AppContext) -> bool {
    extract_user_command_by_number(user_command_number, ctx)
        .map(|c| c.refresh)
        .unwrap_or_default()
}

fn build_external_command_parameters_and_exec_command(
    commit: &Commit,
    refs: &[Ref],
    marked_commit_hash: Option<&CommitHash>,
    user_command_number: usize,
    view_area: Rect,
    ctx: &AppContext,
) -> Result<String, String> {
    build_external_command_parameters(
        commit,
        refs,
        marked_commit_hash,
        user_command_number,
        view_area,
        ctx,
    )
    .and_then(exec_user_command)
}

fn build_external_command_parameters<'a>(
    commit: &'a Commit,
    refs: &'a [Ref],
    marked_commit_hash: Option<&'a CommitHash>,
    user_command_number: usize,
    view_area: Rect,
    ctx: &'a AppContext,
) -> Result<ExternalCommandParameters<'a>, String> {
    let command = &extract_user_command_by_number(user_command_number, ctx)?.commands;
    let target_hash = commit.commit_hash.as_str();
    let parent_hashes = commit
        .parent_commit_hashes
        .iter()
        .map(|c| c.as_str())
        .collect();

    let mut all_refs = vec![];
    let mut branches = vec![];
    let mut remote_branches = vec![];
    let mut tags = vec![];
    let mut stash = None;
    for r in refs {
        match r {
            Ref::Tag { .. } => tags.push(r.name()),
            Ref::Branch { .. } => branches.push(r.name()),
            Ref::RemoteBranch { .. } => remote_branches.push(r.name()),
            Ref::Stash { .. } => {
                stash = Some(r.name());
                continue; // skip stashes from {{refs}}
            }
        }
        all_refs.push(r.name());
    }

    let area_width = view_area.width.saturating_sub(4); // minus the left and right padding
    let area_height = (view_area.height.saturating_sub(1))
        .min(ctx.ui_config.user_command.height)
        .saturating_sub(1); // minus the top border
    Ok(ExternalCommandParameters {
        command,
        target_hash,
        marked_hash: marked_commit_hash.map(CommitHash::as_str),
        parent_hashes,
        all_refs,
        branches,
        remote_branches,
        tags,
        stash,
        area_width,
        area_height,
    })
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, style::Color};
    use rstest::rstest;

    use super::*;
    use crate::{
        config::GraphColorConfig,
        git::SortCommit,
        graph::{calc_graph, CellWidthType, GraphImageWidthMode, GraphStyle},
        test_support::git::GitRepository,
    };

    fn with_deletion_app(f: impl FnOnce(&mut App<'_>)) {
        with_deletion_app_setup(|_| {}, f);
    }

    fn with_deletion_app_setup(setup: impl FnOnce(&std::path::Path), f: impl FnOnce(&mut App<'_>)) {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRepository::new(dir.path());
        git.init();
        git.commit("initial", "2026-01-01");
        git.checkout_b("topic");
        git.commit("unmerged", "2026-01-02");
        git.checkout("master");
        setup(dir.path());
        let repository =
            Repository::load(dir.path(), SortCommit::Chronological, None, false).unwrap();
        let graph = calc_graph(&repository, None);
        let colors = GraphColorSet::new(&GraphColorConfig::default());
        let images = GraphImageManager::new(
            &graph,
            &colors,
            CellWidthType::Double,
            GraphStyle::Rounded,
            false,
            GraphImageWidthMode::Compact,
            ImageProtocol::Iterm2,
        );
        let ctx = Rc::new(AppContext {
            keybind: KeyBind::new(None),
            core_config: CoreConfig::default(),
            ui_config: UiConfig::default(),
            color_theme: ColorTheme::default(),
            image_protocol: ImageProtocol::Iterm2,
        });
        let ec = EventController::without_input();
        let mut app = App::new(
            &repository,
            images,
            &colors,
            InitialSelection::Latest,
            ctx,
            &ec,
        );
        f(&mut app);
    }

    #[test]
    fn test_delete_requires_confirmation_and_cancel_keeps_branch() {
        with_deletion_app(|app| {
            let target = BranchTarget::Local("topic".into());
            app.request_delete_branch(target.clone(), false);
            let pending = app.app_status.pending_prompt.take().unwrap();
            assert!(matches!(
                pending,
                PendingPrompt::ConfirmDelete { force: false, .. }
            ));
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Esc));
            assert!(app.app_status.pending_prompt.is_none());
            assert!(app.repository.check_branch_deletion(&target).is_ok());

            app.request_delete_branch(target.clone(), false);
            let pending = app.app_status.pending_prompt.take().unwrap();
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Char('y')));
            assert!(
                matches!(&app.app_status.status_line, StatusLine::NotificationWarn(text) if text.contains("not fully merged"))
            );
            assert!(app.app_status.pending_prompt.is_none());
            assert!(app.repository.check_branch_deletion(&target).is_ok());

            // Force is a new explicit request, and still needs its own confirmation.
            app.request_delete_branch(target.clone(), true);
            let pending = app.app_status.pending_prompt.take().unwrap();
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Char('n')));
            assert!(app.repository.check_branch_deletion(&target).is_ok());
            app.request_delete_branch(target.clone(), true);
            let pending = app.app_status.pending_prompt.take().unwrap();
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Char('y')));
            assert!(app.repository.check_branch_deletion(&target).is_err());
            assert!(app
                .app_status
                .pending_refresh_message
                .as_ref()
                .unwrap()
                .contains("Deleted local"));
        });
    }

    #[test]
    fn test_delete_picker_pages_and_requires_confirmation_after_selection() {
        with_deletion_app(|app| {
            let mut targets: Vec<_> = (0..9)
                .map(|i| BranchTarget::Local(format!("missing-{i}")))
                .collect();
            targets.push(BranchTarget::Local("topic".into()));
            app.choose_branch_to_delete(targets, false, 0);
            let pending = app.app_status.pending_prompt.take().unwrap();
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Char('n')));
            assert!(matches!(
                app.app_status.pending_prompt,
                Some(PendingPrompt::ChooseBranchToDelete { offset: 9, .. })
            ));
            let pending = app.app_status.pending_prompt.take().unwrap();
            app.answer_prompt(pending, KeyEvent::from(KeyCode::Char('1')));
            assert!(
                matches!(&app.app_status.pending_prompt, Some(PendingPrompt::ConfirmDelete { target: BranchTarget::Local(name), force: false }) if name == "topic")
            );
            assert!(app
                .repository
                .check_branch_deletion(&BranchTarget::Local("topic".into()))
                .is_ok());
        });
    }

    fn git_stdout(path: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    fn ref_target(app: &App<'_>, name: &str) -> CommitHash {
        app.repository
            .all_refs()
            .into_iter()
            .find(|r| r.name() == name)
            .unwrap()
            .target()
            .clone()
    }

    #[test]
    fn test_force_delete_at_commit_offers_only_local_branches() {
        let setup = |path: &std::path::Path| {
            let topic = git_stdout(path, &["rev-parse", "topic"]);
            git_stdout(path, &["update-ref", "refs/remotes/origin/topic", &topic]);
            let tree = git_stdout(path, &["rev-parse", "master^{tree}"]);
            let gone = git_stdout(path, &["commit-tree", &tree, "-p", "master", "-m", "gone"]);
            git_stdout(path, &["update-ref", "refs/remotes/origin/gone", &gone]);
        };
        with_deletion_app_setup(setup, |app| {
            // Safe deletion offers both; force skips straight to the local branch.
            let topic = ref_target(app, "topic");
            app.delete_branch_at_commit(&topic, false);
            assert!(matches!(
                &app.app_status.pending_prompt,
                Some(PendingPrompt::ChooseBranchToDelete { targets, .. }) if targets.len() == 2
            ));
            app.app_status.pending_prompt = None;
            app.delete_branch_at_commit(&topic, true);
            assert!(
                matches!(&app.app_status.pending_prompt, Some(PendingPrompt::ConfirmDelete { target: BranchTarget::Local(name), force: true }) if name == "topic")
            );

            app.app_status.pending_prompt = None;
            let gone = ref_target(app, "origin/gone");
            app.delete_branch_at_commit(&gone, true);
            assert!(app.app_status.pending_prompt.is_none());
            assert!(
                matches!(&app.app_status.status_line, StatusLine::NotificationWarn(text) if text.contains("not supported"))
            );
        });
    }

    #[test]
    fn test_delete_current_branch_and_remote_force_never_prompt() {
        with_deletion_app(|app| {
            app.request_delete_branch(BranchTarget::Local("master".into()), true);
            assert!(app.app_status.pending_prompt.is_none());
            assert!(
                matches!(&app.app_status.status_line, StatusLine::NotificationError(text) if text.contains("checked out"))
            );
            app.request_delete_branch(BranchTarget::Remote("origin/topic".into()), true);
            assert!(app.app_status.pending_prompt.is_none());
            assert!(
                matches!(&app.app_status.status_line, StatusLine::NotificationWarn(text) if text.contains("not supported"))
            );
        });
    }

    #[rstest]
    #[case(1)]
    #[case(2)]
    fn test_fullscreen_clear_redraws_unchanged_content(#[case] frames_before_clear: usize) {
        let mut terminal = Terminal::new(TestBackend::new(5, 1)).unwrap();
        let draw = |terminal: &mut Terminal<TestBackend>| {
            terminal
                .draw(|frame| frame.render_widget(Paragraph::new("hello"), frame.area()))
                .unwrap();
        };
        for _ in 0..frames_before_clear {
            draw(&mut terminal);
        }

        clear_fullscreen(&mut terminal).unwrap();
        assert_eq!(terminal.backend().buffer(), &Buffer::with_lines(["     "]));
        for _ in 0..2 {
            draw(&mut terminal);
            assert_eq!(terminal.backend().buffer(), &Buffer::with_lines(["hello"]));
        }
    }

    #[rustfmt::skip]
    #[rstest]
    #[case("",    UserEvent::NavigateDown, UserEventWithCount::new(UserEvent::NavigateDown, 1))] // no prefix
    #[case("5",   UserEvent::NavigateUp,   UserEventWithCount::new(UserEvent::NavigateUp, 5))] // with prefix
    #[case("0",   UserEvent::PageDown,     UserEventWithCount::new(UserEvent::PageDown, 1))] // zero should be converted to 1
    #[case("42",  UserEvent::ScrollDown,   UserEventWithCount::new(UserEvent::ScrollDown, 42))] // multi-digit number
    #[case("999", UserEvent::PageDown,     UserEventWithCount::new(UserEvent::PageDown, 999))] // large number
    #[case("abc", UserEvent::ScrollUp,     UserEventWithCount::new(UserEvent::ScrollUp, 1))] // should fallback to 1
    #[case("5",   UserEvent::Quit,         UserEventWithCount::new(UserEvent::Quit, 1))] // non-countable event with prefix
    #[case("",    UserEvent::Confirm,      UserEventWithCount::new(UserEvent::Confirm, 1))] // non-countable event without prefix
    fn test_process_numeric_prefix(
        #[case] numeric_prefix: &str,
        #[case] user_event: UserEvent,
        #[case] expected: UserEventWithCount,
    ) {
        let dummy_key_event = KeyEvent::from(KeyCode::Enter); // KeyEvent is not used in the logic
        let actual = process_numeric_prefix(numeric_prefix, user_event, dummy_key_event);
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case("/", 1, "|", "/|")]
    #[case("/abc", 1, "|", "/|abc")]
    #[case("/abc", 2, "|", "/a|bc")]
    #[case("/abc", 4, "|", "/abc|")]
    #[case("/a界b", 2, "|", "/a|界b")]
    #[case("/a界b", 5, "|", "/a界|b")]
    #[case("/ae\u{301}b", 5, "|", "/ae\u{301}|b")]
    #[case("/a👩‍💻b", 13, "|", "/a👩‍💻|b")]
    #[case("/abc", 2, "||", "/a||bc")]
    #[case("/abc", 2, "界", "/a界bc")]
    #[case("/abc", 2, "", "/abc")]
    fn test_status_input_line_inserts_virtual_cursor_without_hiding_text(
        #[case] message: &str,
        #[case] cursor_byte_index: usize,
        #[case] cursor: &str,
        #[case] expected: &str,
    ) {
        let cursor_type = CursorType::Virtual(cursor.into());
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).unwrap();
        terminal
            .draw(|frame| {
                let line = status_input_line(
                    message,
                    cursor_byte_index,
                    &cursor_type,
                    Style::default(),
                    Style::default(),
                );
                frame.render_widget(Paragraph::new(line), frame.area());
            })
            .unwrap();

        let mut expected_buffer = Buffer::with_lines([expected]);
        expected_buffer.resize(Rect::new(0, 0, 20, 1));
        assert_eq!(terminal.backend().buffer(), &expected_buffer);
    }

    #[rstest]
    #[case(16, "/a||bc  [meta]")]
    #[case(15, "/a||bc")]
    #[case(5, "/a||bc")]
    fn test_status_input_metadata_uses_cursor_width_and_preserves_styles(
        #[case] width: u16,
        #[case] expected: &str,
    ) {
        let cursor_type = CursorType::Virtual("||".into());
        let line = status_input_line(
            "/abc",
            2,
            &cursor_type,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
        );
        let line =
            status_line_with_metadata(line, "[meta]", Style::default().fg(Color::Blue), width);
        assert_eq!(line.to_string(), expected);

        let mut terminal = Terminal::new(TestBackend::new(width, 2)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(line).block(
                        Block::default()
                            .borders(Borders::TOP)
                            .padding(Padding::horizontal(1)),
                    ),
                    frame.area(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(1, 1)].fg, Color::Green);
        assert!(buffer[(1, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(3, 1)].fg, Color::Red);
        assert!(!buffer[(3, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(width - 1, 1)].symbol(), " ");
        if width == 16 {
            assert_eq!(buffer[(5, 1)].fg, Color::Green);
            assert_eq!(buffer[(9, 1)].symbol(), "[");
            assert_eq!(buffer[(9, 1)].fg, Color::Blue);
            assert!(!buffer[(9, 1)].modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn test_status_line_with_metadata_right_aligns_metadata() {
        let line = status_line_with_metadata(Line::raw("left"), "[meta]", Style::default(), 14);

        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].content.as_ref(), "left");
        assert_eq!(line.spans[1].content.as_ref(), "  ");
        assert_eq!(line.spans[2].content.as_ref(), "[meta]");
    }

    #[test]
    fn test_status_line_with_metadata_hides_metadata_when_area_is_too_narrow() {
        let line = status_line_with_metadata(Line::raw("left"), "[meta]", Style::default(), 13);

        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content.as_ref(), "left");
    }
}
