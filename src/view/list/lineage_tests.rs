use std::path::PathBuf;

use ratatui::{backend::TestBackend, crossterm::event::KeyCode, style::Color, Terminal};
use rustc_hash::FxHashMap;

use super::*;
use crate::{
    color::{ColorTheme, GraphColorSet},
    config::{CoreConfig, GraphColorConfig, UiConfig},
    event::EventController,
    git::{Commit, Head, Ref, Repository},
    graph::{calc_graph, CellWidthType, GraphImageManager, GraphImageWidthMode, GraphStyle},
    keybind::KeyBind,
    protocol::ImageProtocol,
    search::SearchOptions,
    widget::commit_list::CommitInfo,
};

fn hash(index: usize) -> CommitHash {
    CommitHash::from(format!("{:040x}", index + 1).as_str())
}

fn with_view(f: impl FnOnce(&mut ListView<'_>)) {
    // D merges B and C. Only B is D's first-parent lineage.
    let parents = [vec![2, 1], vec![4], vec![3], vec![4], vec![5], vec![]];
    let subjects = ["D", "C", "B", "linear", "A", "root"];
    let commits: Vec<_> = subjects
        .iter()
        .enumerate()
        .map(|(index, subject)| Commit {
            commit_hash: hash(index),
            subject: (*subject).into(),
            parent_commit_hashes: parents[index].iter().map(|&index| hash(index)).collect(),
            ..Commit::default()
        })
        .collect();
    let mut parents_map = FxHashMap::default();
    let mut children_map = FxHashMap::<CommitHash, Vec<CommitHash>>::default();
    for commit in &commits {
        parents_map.insert(
            commit.commit_hash.clone(),
            commit.parent_commit_hashes.clone(),
        );
        for parent in &commit.parent_commit_hashes {
            children_map
                .entry(parent.clone())
                .or_default()
                .push(commit.commit_hash.clone());
        }
    }
    let refs = (0..3)
        .map(|index| {
            (
                hash(index),
                vec![Ref::Branch {
                    name: subjects[index].into(),
                    target: hash(index),
                }],
            )
        })
        .collect();
    let repository = Repository::new(
        PathBuf::new(),
        commits
            .into_iter()
            .map(|commit| (commit.commit_hash.clone(), commit))
            .collect(),
        parents_map,
        children_map,
        refs,
        Head::None,
        (0..subjects.len()).map(hash).collect(),
    );
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
    let infos = graph
        .commits
        .iter()
        .map(|commit| CommitInfo::new(commit, repository.refs(&commit.commit_hash), Color::Reset))
        .collect();
    let state = CommitListState::new(
        infos,
        images,
        8,
        repository.head(),
        FxHashMap::default(),
        SearchOptions::default(),
        0,
    );
    let ctx = Rc::new(AppContext {
        keybind: KeyBind::new(None),
        core_config: CoreConfig::default(),
        ui_config: UiConfig::default(),
        color_theme: ColorTheme::default(),
        image_protocol: ImageProtocol::Iterm2,
    });
    let ec = EventController::without_input();
    let mut view = ListView::new(state, ctx, ec.sender());
    view.update_layout(Rect::new(0, 0, 100, 10));
    f(&mut view);
}

fn press(view: &mut ListView<'_>, event: UserEvent) {
    view.handle_event(
        UserEventWithCount::from_event(event),
        KeyEvent::from(KeyCode::Null),
    );
}

fn selected(view: &ListView<'_>, index: usize) {
    assert_eq!(view.as_list_state().selected_commit_hash(), &hash(index));
}

#[test]
fn follows_first_parent_and_remembers_return_path_at_unlabelled_fork() {
    with_view(|view| {
        press(view, UserEvent::LineageRoot);
        selected(view, 2); // B, not merge's second parent C
        press(view, UserEvent::LineageRoot);
        selected(view, 4); // unlabelled fork A, skipping linear
        press(view, UserEvent::LineageTip);
        let picker = view.lineage_picker.as_ref().unwrap();
        assert_eq!(picker.choices.len(), 2);
        assert_eq!(picker.choices[picker.selected].target, 2); // B remembered
        assert!(picker.choices[picker.selected].label.starts_with("B ("));
        selected(view, 4); // chooser doesn't move the graph before confirmation
        press(view, UserEvent::Confirm);
        selected(view, 2);
        assert!(view.lineage_picker.is_none());
        press(view, UserEvent::LineageTip);
        selected(view, 0); // unique continuation to D, no chooser
    });
}

#[test]
fn chooser_can_switch_siblings_cancel_and_consume_other_commands() {
    with_view(|view| {
        view.as_mut_list_state().select_commit_hash(&hash(4));
        press(view, UserEvent::LineageTip);
        assert_eq!(view.lineage_picker.as_ref().unwrap().selected, 0);
        press(view, UserEvent::NavigateDown);
        assert_eq!(view.lineage_picker.as_ref().unwrap().selected, 1);
        press(view, UserEvent::NavigateDown); // wraps to C
        assert_eq!(view.lineage_picker.as_ref().unwrap().selected, 0);
        press(view, UserEvent::Refresh); // no refresh/checkout while choosing
        assert!(view.lineage_picker.is_some());
        press(view, UserEvent::Cancel);
        selected(view, 4);
        assert!(view.lineage_picker.is_none());
        press(view, UserEvent::LineageTip);
        press(view, UserEvent::Confirm); // choose C
        selected(view, 1);
        press(view, UserEvent::LineageTip);
        selected(view, 1); // D is a merge child, not a first-parent child
        press(view, UserEvent::LineageRoot);
        selected(view, 4);
        press(view, UserEvent::LineageTip);
        let picker = view.lineage_picker.as_ref().unwrap();
        assert_eq!(picker.choices[picker.selected].target, 1);
    });
}

#[test]
fn endpoints_do_not_wrap_and_narrow_rendering_is_safe() {
    with_view(|view| {
        press(view, UserEvent::LineageTip);
        selected(view, 0);
        view.as_mut_list_state().select_commit_hash(&hash(5));
        press(view, UserEvent::LineageRoot);
        selected(view, 5);
        press(view, UserEvent::LineageTip);
        selected(view, 4);
        press(view, UserEvent::LineageTip);
        for (width, height) in [(100, 10), (8, 2), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            view.update_layout(Rect::new(0, 0, width, height));
            view.prepare_graph_uploads();
            terminal
                .draw(|frame| view.render(frame, frame.area()))
                .unwrap();
        }
        assert!(view.lineage_picker.is_some());
    });
}

#[test]
fn active_search_treats_brackets_as_query_characters() {
    with_view(|view| {
        press(view, UserEvent::Search);
        view.handle_event(
            UserEventWithCount::from_event(UserEvent::LineageTip),
            KeyEvent::from(KeyCode::Char(']')),
        );
        assert!(view
            .as_list_state()
            .search_query_string()
            .unwrap()
            .contains(']'));
        assert!(view.lineage_picker.is_none());
        selected(view, 0);
    });
}

#[test]
fn labels_for_unlabelled_landmarks_include_subject_and_hash() {
    with_view(|view| {
        view.as_mut_list_state().select_commit_hash(&hash(5));
        let (choices, _) = view.as_list_state().lineage_choices();
        assert_eq!(choices.len(), 1);
        assert!(choices[0].label.ends_with(" A"));
        assert!(choices[0].label.starts_with(hash(4).as_short_hash()));
    });
}
