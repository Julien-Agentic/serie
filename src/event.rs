use std::{
    fmt::{self, Debug, Formatter},
    sync::mpsc,
};

use ratatui::crossterm::event::KeyEvent;
use serde::{
    de::{self, Deserializer, Visitor},
    Deserialize,
};

use crate::{
    git::{BranchTarget, CommitHash},
    view::RefreshViewContext,
};

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(usize, usize),
    Quit,
    OpenDetail,
    CloseDetail,
    OpenFile(String),
    OpenUserCommand(usize),
    CloseUserCommand,
    OpenRefs,
    CloseRefs,
    OpenWorktrees,
    CloseWorktrees,
    OpenWorktree(std::path::PathBuf),
    OpenHelp,
    CloseHelp,
    SelectNewerCommit,
    SelectOlderCommit,
    SelectParentCommit,
    SwitchBranch(BranchTarget),
    SwitchBranchAtSelectedCommit,
    CopyBranchName(CommitHash),
    DeleteBranch {
        target: BranchTarget,
        force: bool,
    },
    DeleteBranchAtCommit {
        hash: CommitHash,
        force: bool,
    },
    ResizeGraph {
        wider: bool,
        steps: usize,
    },
    CopyToClipboard {
        name: String,
        value: String,
    },
    Refresh(RefreshViewContext),
    AutoRefresh(crate::git::RefSnapshot, RefreshViewContext),
    ClearStatusLine,
    UpdateStatusInput {
        message: String,
        cursor_byte_index: usize,
        metadata: String,
    },
    UpdateStatusTransient(String),
    UpdateSearchResult {
        message: String,
        options: String,
        matched: bool,
    },
    NotifyInfo(String),
    NotifySuccess(String),
    #[expect(dead_code)]
    NotifyWarn(String),
    NotifyError(String),
}

#[derive(Clone)]
pub struct Sender {
    tx: mpsc::Sender<AppEvent>,
}

impl Sender {
    pub fn send(&self, event: AppEvent) {
        self.tx.send(event).unwrap();
    }
}

impl Debug for Sender {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Sender")
    }
}

pub struct Receiver {
    rx: mpsc::Receiver<AppEvent>,
}

impl Receiver {
    fn recv(&self) -> AppEvent {
        self.rx.recv().unwrap()
    }
}

impl Debug for Receiver {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Receiver")
    }
}

#[derive(Debug)]
pub struct EventController {
    tx: Sender,
    rx: Receiver,
    read_input: bool,
}

impl EventController {
    #[cfg(test)]
    pub fn without_input() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx: Sender { tx },
            rx: Receiver { rx },
            read_input: false,
        }
    }

    pub fn init() -> Self {
        let (tx, rx) = mpsc::channel();
        let tx = Sender { tx };
        let rx = Receiver { rx };

        EventController {
            tx,
            rx,
            read_input: true,
        }
    }

    pub fn resume(&self) {
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::EnterAlternateScreen
        )
        .unwrap();
        ratatui::crossterm::terminal::enable_raw_mode().unwrap();

        self.drain_crossterm_event();
    }

    pub fn suspend(&self) {
        // Input is read only by recv(), on the application thread. There is no
        // background poll to join or reader competing with the external app.
        ratatui::crossterm::terminal::disable_raw_mode().unwrap();
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::LeaveAlternateScreen
        )
        .unwrap();
    }

    fn drain_crossterm_event(&self) {
        while let Ok(true) = ratatui::crossterm::event::poll(std::time::Duration::from_millis(0)) {
            let _ = ratatui::crossterm::event::read();
        }
    }

    pub fn sender(&self) -> Sender {
        self.tx.clone()
    }

    pub fn send(&self, event: AppEvent) {
        self.tx.send(event);
    }

    #[cfg(test)]
    pub fn recv(&self) -> AppEvent {
        self.recv_with_background(|| None)
    }

    pub fn recv_with_background(
        &self,
        mut background: impl FnMut() -> Option<AppEvent>,
    ) -> AppEvent {
        if !self.read_input {
            return self.rx.recv();
        }
        self.recv_with(|| {
            if let Some(event) = background() {
                return Some(event);
            }
            if !ratatui::crossterm::event::poll(std::time::Duration::from_millis(100))
                .expect("Failed to poll event")
            {
                return None;
            }
            match ratatui::crossterm::event::read().expect("Failed to read event") {
                ratatui::crossterm::event::Event::Key(key) => Some(AppEvent::Key(key)),
                ratatui::crossterm::event::Event::Resize(w, h) => {
                    Some(AppEvent::Resize(w as usize, h as usize))
                }
                _ => None,
            }
        })
    }

    fn recv_with(&self, mut read_input: impl FnMut() -> Option<AppEvent>) -> AppEvent {
        loop {
            // Finish actions queued by the last key before reading another one.
            // The timeout in recv bounds delivery of future background messages;
            // it does not delay input or handing the terminal to another app.
            if let Ok(event) = self.rx.rx.try_recv() {
                return event;
            }
            if let Some(event) = read_input() {
                return event;
            }
        }
    }
}

// The event triggered by user's key input
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserEvent {
    ForceQuit,
    Quit,
    HelpToggle,
    Cancel,
    Close,
    NavigateUp,
    NavigateDown,
    NavigateRight,
    NavigateLeft,
    SelectUp,
    SelectDown,
    GoToTop,
    GoToBottom,
    GoToParent,
    GoToNextBranch,
    GoToPreviousBranch,
    LineageRoot,
    LineageTip,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    SelectTop,
    SelectMiddle,
    SelectBottom,
    GoToNext,
    GoToPrevious,
    Confirm,
    RefList,
    WorktreeList,
    EditFile,
    Search,
    UserCommand(usize),
    SearchTargetToggle,
    IgnoreCaseToggle,
    FuzzyToggle,
    MarkToggle,
    Refresh,
    SwitchBranch,
    GraphNarrower,
    GraphWider,
    ShortCopy,
    FullCopy,
    CopyBranchName,
    DeleteBranch,
    ForceDeleteBranch,
    Unknown,
}

impl<'de> Deserialize<'de> for UserEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UserEventVisitor;

        impl<'de> Visitor<'de> for UserEventVisitor {
            type Value = UserEvent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing a user event")
            }

            fn visit_str<E>(self, value: &str) -> Result<UserEvent, E>
            where
                E: de::Error,
            {
                if value.starts_with("user_command_") {
                    if let Some(num) = parse_user_command_number(value) {
                        Ok(UserEvent::UserCommand(num))
                    } else {
                        let msg = format!("Invalid user_command_n format: {value}",);
                        Err(de::Error::custom(msg))
                    }
                } else {
                    match value {
                        "force_quit" => Ok(UserEvent::ForceQuit),
                        "quit" => Ok(UserEvent::Quit),
                        "help_toggle" => Ok(UserEvent::HelpToggle),
                        "cancel" => Ok(UserEvent::Cancel),
                        "close" => Ok(UserEvent::Close),
                        "navigate_up" => Ok(UserEvent::NavigateUp),
                        "navigate_down" => Ok(UserEvent::NavigateDown),
                        "navigate_right" => Ok(UserEvent::NavigateRight),
                        "navigate_left" => Ok(UserEvent::NavigateLeft),
                        "select_up" => Ok(UserEvent::SelectUp),
                        "select_down" => Ok(UserEvent::SelectDown),
                        "go_to_top" => Ok(UserEvent::GoToTop),
                        "go_to_bottom" => Ok(UserEvent::GoToBottom),
                        "go_to_parent" => Ok(UserEvent::GoToParent),
                        "go_to_next_branch" => Ok(UserEvent::GoToNextBranch),
                        "go_to_previous_branch" => Ok(UserEvent::GoToPreviousBranch),
                        "lineage_root" => Ok(UserEvent::LineageRoot),
                        "lineage_tip" => Ok(UserEvent::LineageTip),
                        "scroll_up" => Ok(UserEvent::ScrollUp),
                        "scroll_down" => Ok(UserEvent::ScrollDown),
                        "page_up" => Ok(UserEvent::PageUp),
                        "page_down" => Ok(UserEvent::PageDown),
                        "half_page_up" => Ok(UserEvent::HalfPageUp),
                        "half_page_down" => Ok(UserEvent::HalfPageDown),
                        "select_top" => Ok(UserEvent::SelectTop),
                        "select_middle" => Ok(UserEvent::SelectMiddle),
                        "select_bottom" => Ok(UserEvent::SelectBottom),
                        "go_to_next" => Ok(UserEvent::GoToNext),
                        "go_to_previous" => Ok(UserEvent::GoToPrevious),
                        "confirm" => Ok(UserEvent::Confirm),
                        "ref_list" | "ref_list_toggle" => Ok(UserEvent::RefList),
                        "worktree_list" => Ok(UserEvent::WorktreeList),
                        "edit_file" => Ok(UserEvent::EditFile),
                        "search" => Ok(UserEvent::Search),
                        "search_target_toggle" => Ok(UserEvent::SearchTargetToggle),
                        "ignore_case_toggle" => Ok(UserEvent::IgnoreCaseToggle),
                        "fuzzy_toggle" => Ok(UserEvent::FuzzyToggle),
                        "mark_toggle" => Ok(UserEvent::MarkToggle),
                        "refresh" => Ok(UserEvent::Refresh),
                        "switch_branch" => Ok(UserEvent::SwitchBranch),
                        "graph_narrower" => Ok(UserEvent::GraphNarrower),
                        "graph_wider" => Ok(UserEvent::GraphWider),
                        "short_copy" => Ok(UserEvent::ShortCopy),
                        "full_copy" => Ok(UserEvent::FullCopy),
                        "copy_branch_name" => Ok(UserEvent::CopyBranchName),
                        "delete_branch" => Ok(UserEvent::DeleteBranch),
                        "force_delete_branch" => Ok(UserEvent::ForceDeleteBranch),
                        _ => {
                            let msg = format!("Unknown user event: {value}");
                            Err(de::Error::custom(msg))
                        }
                    }
                }
            }
        }

        deserializer.deserialize_str(UserEventVisitor)
    }
}

fn parse_user_command_number(s: &str) -> Option<usize> {
    if let Some(num_str) = s.strip_prefix("user_command_") {
        if num_str.parse::<usize>().is_ok() {
            return num_str.parse::<usize>().ok();
        }
        if let Some(num_str) = s.strip_prefix("user_command_view_toggle_") {
            if num_str.parse::<usize>().is_ok() {
                return num_str.parse::<usize>().ok();
            }
        }
    }
    None
}

impl UserEvent {
    pub fn is_countable(&self) -> bool {
        matches!(
            self,
            UserEvent::NavigateUp
                | UserEvent::NavigateDown
                | UserEvent::ScrollUp
                | UserEvent::ScrollDown
                | UserEvent::GoToParent
                | UserEvent::GoToNextBranch
                | UserEvent::GoToPreviousBranch
                | UserEvent::PageUp
                | UserEvent::PageDown
                | UserEvent::HalfPageUp
                | UserEvent::HalfPageDown
                | UserEvent::GraphNarrower
                | UserEvent::GraphWider
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserEventWithCount {
    pub event: UserEvent,
    pub count: usize,
}

impl UserEventWithCount {
    pub fn new(event: UserEvent, count: usize) -> Self {
        Self {
            event,
            count: if count == 0 { 1 } else { count },
        }
    }

    pub fn from_event(event: UserEvent) -> Self {
        Self::new(event, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queued_action_is_processed_before_more_terminal_input() {
        let controller = EventController::without_input();
        controller.send(AppEvent::OpenUserCommand(2));
        let event = controller.recv_with(|| panic!("must not read ahead of a queued action"));
        assert!(matches!(event, AppEvent::OpenUserCommand(2)));
    }

    #[test]
    fn test_input_is_not_read_ahead_after_launch_key() {
        let controller = EventController::without_input();
        let mut reads = 0;
        let event = controller.recv_with(|| {
            reads += 1;
            Some(AppEvent::Key(KeyEvent::from(
                ratatui::crossterm::event::KeyCode::Char('V'),
            )))
        });
        assert!(matches!(event, AppEvent::Key(_)));
        assert_eq!(reads, 1);
        controller.send(AppEvent::OpenUserCommand(2));
        assert!(matches!(
            controller.recv_with(|| panic!("no competing input reader during launch")),
            AppEvent::OpenUserCommand(2)
        ));
    }

    #[test]
    fn test_background_event_delivered_after_input_poll_timeout() {
        let controller = EventController::without_input();
        let mut polls = 0;
        let event = controller.recv_with(|| {
            polls += 1;
            controller.send(AppEvent::Resize(80, 24));
            None
        });
        assert!(matches!(event, AppEvent::Resize(80, 24)));
        assert_eq!(polls, 1);
    }

    #[test]
    fn test_user_event_with_count_new() {
        let event = UserEventWithCount::new(UserEvent::NavigateUp, 5);
        assert_eq!(event.event, UserEvent::NavigateUp);
        assert_eq!(event.count, 5);
    }

    #[test]
    fn test_user_event_with_count_new_zero_count() {
        let event = UserEventWithCount::new(UserEvent::NavigateDown, 0);
        assert_eq!(event.event, UserEvent::NavigateDown);
        assert_eq!(event.count, 1); // zero should be converted to 1
    }

    #[test]
    fn test_user_event_with_count_from_event() {
        let event = UserEventWithCount::from_event(UserEvent::NavigateLeft);
        assert_eq!(event.event, UserEvent::NavigateLeft);
        assert_eq!(event.count, 1);
    }

    #[test]
    fn test_user_event_with_count_equality() {
        let event1 = UserEventWithCount::new(UserEvent::ScrollUp, 3);
        let event2 = UserEventWithCount::new(UserEvent::ScrollUp, 3);
        let event3 = UserEventWithCount::new(UserEvent::ScrollDown, 3);

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
    }
}
