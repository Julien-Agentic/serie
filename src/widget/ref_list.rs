use std::{collections::HashSet, rc::Rc};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Padding, StatefulWidget},
};
use semver::Version;

use crate::{app::AppContext, git::Ref};

const TREE_BRANCH_ROOT_IDENT: &str = "__branches__";
const TREE_REMOTE_ROOT_IDENT: &str = "__remotes__";
const TREE_TAG_ROOT_IDENT: &str = "__tags__";
const TREE_STASH_ROOT_IDENT: &str = "__stashes__";

const TREE_BRANCH_ROOT_TEXT: &str = "Branches";
const TREE_REMOTE_ROOT_TEXT: &str = "Remotes";
const TREE_TAG_ROOT_TEXT: &str = "Tags";
const TREE_STASH_ROOT_TEXT: &str = "Stashes";

#[derive(Debug)]
pub struct RefListState {
    roots: Vec<RefTreeNode>,
    visible_rows: Vec<VisibleRefRow>,
    selected: Vec<String>,
    opened: HashSet<Vec<String>>,
    offset: usize,
    scroll_to_selection: bool,

    current_branch: Option<String>,
    other_worktree_branches: HashSet<String>,
}

impl RefListState {
    pub fn new(refs: &[&Ref]) -> Self {
        let selected = vec![TREE_BRANCH_ROOT_IDENT.into()];
        let opened = HashSet::from([selected.clone()]);
        let mut state = Self {
            roots: build_ref_tree_nodes(refs),
            visible_rows: Vec::new(),
            selected,
            opened,
            offset: 0,
            scroll_to_selection: true,
            current_branch: None,
            other_worktree_branches: HashSet::new(),
        };
        state.rebuild_visible_rows();
        state
    }

    /// Sets which local branch is checked out here and which are checked out in
    /// other worktrees, so that the list can mark them.
    pub fn set_branch_status(
        &mut self,
        current_branch: Option<String>,
        other_worktree_branches: HashSet<String>,
    ) {
        self.current_branch = current_branch;
        self.other_worktree_branches = other_worktree_branches;
    }

    pub fn select_next(&mut self) {
        let next = self
            .selected_index()
            .map_or(0, |index| index.saturating_add(1))
            .min(self.visible_rows.len().saturating_sub(1));
        self.select_visible_index(next);
    }

    pub fn select_prev(&mut self) {
        let prev = self
            .selected_index()
            .map_or(self.visible_rows.len().saturating_sub(1), |index| {
                index.saturating_sub(1)
            });
        self.select_visible_index(prev);
    }

    pub fn select_first(&mut self) {
        self.select_visible_index(0);
    }

    pub fn select_last(&mut self) {
        self.select_visible_index(self.visible_rows.len().saturating_sub(1));
    }

    pub fn open_node(&mut self) {
        if self
            .visible_rows
            .iter()
            .any(|row| row.identifier == self.selected && row.has_children)
            && self.opened.insert(self.selected.clone())
        {
            self.rebuild_visible_rows();
            self.scroll_to_selection = true;
        }
    }

    pub fn close_node(&mut self) {
        if self.opened.remove(&self.selected) {
            self.rebuild_visible_rows();
        } else if self.selected.len() > 1 {
            self.selected.pop();
        }
        // The four category roots always remain selectable.
        self.scroll_to_selection = true;
    }

    pub fn selected_ref_name(&self) -> Option<String> {
        self.selected.last().cloned()
    }

    pub fn selected_branch(&self) -> Option<String> {
        if self.selected.len() > 1
            && (self.selected[0] == TREE_BRANCH_ROOT_IDENT
                || self.selected[0] == TREE_REMOTE_ROOT_IDENT)
        {
            self.selected.last().cloned()
        } else {
            None
        }
    }

    /// The selected local branch, unless the selection is a folder of branches.
    pub fn selected_local_branch(&self) -> Option<String> {
        self.selected_leaf_under(TREE_BRANCH_ROOT_IDENT)
    }

    /// The selected remote-tracking branch, unless the selection is a folder.
    pub fn selected_remote_branch(&self) -> Option<String> {
        self.selected_leaf_under(TREE_REMOTE_ROOT_IDENT)
    }

    fn selected_leaf_under(&self, root: &str) -> Option<String> {
        let row = &self.visible_rows[self.selected_index()?];
        if row.depth > 0 && !row.has_children && row.identifier[0] == root {
            row.identifier.last().cloned()
        } else {
            None
        }
    }

    pub fn selected_tag(&self) -> Option<String> {
        if self.selected.len() > 1 && self.selected[0] == TREE_TAG_ROOT_IDENT {
            self.selected.last().cloned()
        } else {
            None
        }
    }

    pub fn current_tree_status(&self) -> (Vec<String>, Vec<Vec<String>>) {
        (self.selected.clone(), self.opened.iter().cloned().collect())
    }

    pub fn reset_tree_status(&mut self, selected: Vec<String>, opened: Vec<Vec<String>>) {
        self.opened = opened.into_iter().collect();
        self.rebuild_visible_rows();
        self.selected = selected;
        while !self.selected.is_empty() && self.selected_index().is_none() {
            self.selected.pop();
        }
        if self.selected.is_empty() {
            self.selected = vec![TREE_BRANCH_ROOT_IDENT.into()];
        }
        self.scroll_to_selection = true;
    }

    fn selected_index(&self) -> Option<usize> {
        self.visible_rows
            .iter()
            .position(|row| row.identifier == self.selected)
    }

    fn select_visible_index(&mut self, index: usize) {
        if let Some(row) = self.visible_rows.get(index) {
            self.selected.clone_from(&row.identifier);
            self.scroll_to_selection = true;
        }
    }

    fn rebuild_visible_rows(&mut self) {
        self.visible_rows.clear();
        collect_visible_rows(&self.roots, &self.opened, &[], &mut self.visible_rows);
    }
}

#[derive(Debug)]
struct VisibleRefRow {
    identifier: Vec<String>,
    name: String,
    depth: usize,
    has_children: bool,
}

fn collect_visible_rows(
    nodes: &[RefTreeNode],
    opened: &HashSet<Vec<String>>,
    parent: &[String],
    rows: &mut Vec<VisibleRefRow>,
) {
    for node in nodes {
        let mut identifier = parent.to_vec();
        identifier.push(node.identifier.clone());
        rows.push(VisibleRefRow {
            identifier: identifier.clone(),
            name: node.name.clone(),
            depth: parent.len(),
            has_children: !node.children.is_empty(),
        });
        if opened.contains(&identifier) {
            collect_visible_rows(&node.children, opened, &identifier, rows);
        }
    }
}

pub struct RefList {
    ctx: Rc<AppContext>,
}

impl RefList {
    pub fn new(ctx: Rc<AppContext>) -> RefList {
        RefList { ctx }
    }
}

impl StatefulWidget for RefList {
    type State = RefListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = Block::default()
            .borders(Borders::LEFT)
            .style(Style::default().fg(self.ctx.color_theme.divider_fg))
            .padding(Padding::horizontal(1));
        let inner = block.inner(area);
        ratatui::widgets::Widget::render(block, area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let height = inner.height as usize;
        state.offset = state.offset.min(state.visible_rows.len().saturating_sub(1));
        if state.scroll_to_selection {
            if let Some(index) = state.selected_index() {
                if index < state.offset {
                    state.offset = index;
                } else if index >= state.offset + height {
                    state.offset = index + 1 - height;
                }
            }
            state.scroll_to_selection = false;
        }

        let item_style = Style::default().fg(self.ctx.color_theme.fg);
        let highlight_style = Style::default()
            .bg(self.ctx.color_theme.ref_selected_bg)
            .fg(self.ctx.color_theme.ref_selected_fg);

        for (line, row) in state
            .visible_rows
            .iter()
            .skip(state.offset)
            .take(height)
            .enumerate()
        {
            let y = inner.y + line as u16;
            let is_local_branch =
                row.depth > 0 && !row.has_children && row.identifier[0] == TREE_BRANCH_ROOT_IDENT;
            let branch_name = row.identifier.last().filter(|_| is_local_branch);
            let is_current = branch_name.is_some() && branch_name == state.current_branch.as_ref();
            let in_other_worktree =
                branch_name.is_some_and(|name| state.other_worktree_branches.contains(name));
            // The same markers as `git branch`.
            let symbol = if is_current {
                "* "
            } else if in_other_worktree {
                "+ "
            } else if !row.has_children {
                "  "
            } else if state.opened.contains(&row.identifier) {
                "▾ "
            } else {
                "▸ "
            };
            let row_style = if is_current {
                item_style
                    .fg(self.ctx.color_theme.list_head_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                item_style
            };
            let prefix = format!("{}{symbol}", "  ".repeat(row.depth));
            let (text_x, _) = buf.set_stringn(inner.x, y, prefix, inner.width as usize, row_style);
            let remaining = inner.width.saturating_sub(text_x - inner.x);
            buf.set_stringn(text_x, y, &row.name, remaining as usize, row_style);
            if row.identifier == state.selected {
                buf.set_style(Rect::new(inner.x, y, inner.width, 1), highlight_style);
            }
        }
    }
}

fn build_ref_tree_nodes(refs: &[&Ref]) -> Vec<RefTreeNode> {
    let mut branch_refs = Vec::new();
    let mut remote_refs = Vec::new();
    let mut tag_refs = Vec::new();
    let mut stash_refs = Vec::new();

    for &r in refs {
        match r {
            Ref::Tag { name, .. } => tag_refs.push(name.into()),
            Ref::Branch { name, .. } => branch_refs.push(name.into()),
            Ref::RemoteBranch { name, .. } => remote_refs.push(name.into()),
            Ref::Stash { name, message, .. } => stash_refs.push((name.into(), message.into())),
        }
    }

    let mut branch_nodes = refs_to_ref_tree_nodes(branch_refs);
    let mut remote_nodes = refs_to_ref_tree_nodes(remote_refs);
    let mut tag_nodes = refs_to_ref_tree_nodes(tag_refs);
    let mut stash_nodes = refs_to_stash_ref_tree_nodes(stash_refs);

    sort_branch_tree_nodes(&mut branch_nodes);
    sort_branch_tree_nodes(&mut remote_nodes);
    sort_tag_tree_nodes(&mut tag_nodes);
    sort_stash_tree_nodes(&mut stash_nodes);

    vec![
        RefTreeNode::new(
            TREE_BRANCH_ROOT_IDENT.into(),
            TREE_BRANCH_ROOT_TEXT.into(),
            branch_nodes,
        ),
        RefTreeNode::new(
            TREE_REMOTE_ROOT_IDENT.into(),
            TREE_REMOTE_ROOT_TEXT.into(),
            remote_nodes,
        ),
        RefTreeNode::new(
            TREE_TAG_ROOT_IDENT.into(),
            TREE_TAG_ROOT_TEXT.into(),
            tag_nodes,
        ),
        RefTreeNode::new(
            TREE_STASH_ROOT_IDENT.into(),
            TREE_STASH_ROOT_TEXT.into(),
            stash_nodes,
        ),
    ]
}

#[derive(Debug)]
struct RefTreeNode {
    identifier: String,
    name: String,
    children: Vec<RefTreeNode>,
}

impl RefTreeNode {
    fn new(identifier: String, name: String, children: Vec<Self>) -> Self {
        Self {
            identifier,
            name,
            children,
        }
    }
}

fn refs_to_stash_ref_tree_nodes(ref_name_messages: Vec<(String, String)>) -> Vec<RefTreeNode> {
    let mut nodes: Vec<RefTreeNode> = Vec::new();
    for (name, message) in ref_name_messages {
        let node = RefTreeNode {
            identifier: name.clone(),
            name: message.to_string(),
            children: Vec::new(),
        };
        nodes.push(node);
    }
    nodes
}

fn refs_to_ref_tree_nodes(ref_names: Vec<String>) -> Vec<RefTreeNode> {
    let mut nodes: Vec<RefTreeNode> = Vec::new();

    for ref_name in ref_names {
        let mut current_nodes = &mut nodes;
        let mut parent_identifier = String::new();

        for part in ref_name.split('/') {
            if let Some(index) = current_nodes.iter().position(|n| n.name == part) {
                let node = &mut current_nodes[index];
                current_nodes = &mut node.children;
                parent_identifier.clone_from(&node.identifier);
            } else {
                let identifier = if parent_identifier.is_empty() {
                    part.to_string()
                } else {
                    format!("{parent_identifier}/{part}")
                };
                let node = RefTreeNode {
                    identifier: identifier.clone(),
                    name: part.to_string(),
                    children: Vec::new(),
                };
                current_nodes.push(node);
                current_nodes = current_nodes.last_mut().unwrap().children.as_mut();
                parent_identifier = identifier;
            }
        }
    }

    nodes
}

fn sort_branch_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| {
        b.children
            .len()
            .cmp(&a.children.len())
            .then(a.name.cmp(&b.name))
    });
    for node in nodes {
        sort_branch_tree_nodes(&mut node.children);
    }
}

fn sort_tag_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| {
        let a_version = parse_semantic_version_tag(&a.name);
        let b_version = parse_semantic_version_tag(&b.name);
        if a_version.is_none() && b_version.is_none() {
            // if both are not semantic versions, sort by name asc
            a.name.cmp(&b.name)
        } else {
            // if both are semantic versions, sort by version desc
            // if only one is a semantic version, it will be sorted first
            b_version.cmp(&a_version)
        }
    });
}

fn sort_stash_tree_nodes(nodes: &mut [RefTreeNode]) {
    nodes.sort_by(|a, b| a.identifier.cmp(&b.identifier));
}

fn parse_semantic_version_tag(tag: &str) -> Option<Version> {
    let tag = tag.trim_start_matches('v');
    Version::parse(tag).ok()
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::*;
    use crate::{
        config::{CoreConfig, UiConfig},
        keybind::KeyBind,
        protocol::ImageProtocol,
    };

    fn refs() -> Vec<Ref> {
        let target = crate::git::CommitHash::from("0000000000000000000000000000000000000001");
        let branch = |name: &str| Ref::Branch {
            name: name.into(),
            target: target.clone(),
        };
        let remote = |name: &str| Ref::RemoteBranch {
            name: name.into(),
            target: target.clone(),
        };
        vec![
            branch("main"),
            branch("feature/login"),
            branch("feature/logout"),
            branch("fix/typo"),
            remote("origin/main"),
            remote("origin/feature/login"),
            Ref::Tag {
                name: "v1.0.0".into(),
                target: target.clone(),
            },
            Ref::Stash {
                name: "stash@{0}".into(),
                message: "WIP on main: login form".into(),
                target,
            },
        ]
    }

    fn state() -> RefListState {
        let refs = refs();
        RefListState::new(&refs.iter().collect::<Vec<_>>())
    }

    fn render(state: &mut RefListState, width: u16, height: u16) -> Buffer {
        let ctx = Rc::new(AppContext {
            keybind: KeyBind::new(None),
            core_config: CoreConfig::default(),
            ui_config: UiConfig::default(),
            color_theme: crate::color::ColorTheme::default(),
            image_protocol: ImageProtocol::Iterm2,
        });
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        RefList::new(ctx).render(area, &mut buf, state);
        buf
    }

    fn line(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn test_selected_branch_is_none_for_folders_and_other_refs() {
        let mut state = state();
        // Branches root
        assert_eq!(state.selected_local_branch(), None);
        state.select_next(); // feature (folder)
        assert_eq!(state.selected_local_branch(), None);
        assert_eq!(state.selected_remote_branch(), None);

        state.open_node();
        state.select_next(); // feature/login
        assert_eq!(
            state.selected_local_branch().as_deref(),
            Some("feature/login")
        );
        assert_eq!(state.selected_remote_branch(), None);

        state.select_last();
        state.select_prev(); // Tags
        state.open_node();
        state.select_next(); // v1.0.0
        assert_eq!(state.selected_local_branch(), None);
        assert_eq!(state.selected_remote_branch(), None);
        assert_eq!(state.selected_tag().as_deref(), Some("v1.0.0"));
    }

    #[test]
    fn test_selected_remote_branch() {
        let mut state = state();
        for _ in 0..4 {
            state.select_next(); // feature, fix, main, Remotes
        }
        state.open_node();
        state.select_next(); // origin
        state.open_node();
        state.select_next(); // feature (folder)
        assert_eq!(state.selected_remote_branch(), None);
        state.select_next(); // origin/main

        assert_eq!(
            state.selected_remote_branch().as_deref(),
            Some("origin/main")
        );
        assert_eq!(state.selected_local_branch(), None);
    }

    #[test]
    fn test_render_marks_current_and_other_worktree_branches() {
        let mut state = state();
        state.set_branch_status(
            Some("main".into()),
            HashSet::from(["feature/login".to_string()]),
        );
        state.select_next(); // feature (folder)
        state.open_node();

        let buf = render(&mut state, 30, 10);

        assert_eq!(line(&buf, 0), "│ ▾ Branches");
        assert_eq!(line(&buf, 1), "│   ▾ feature");
        assert_eq!(line(&buf, 2), "│     + login");
        assert_eq!(line(&buf, 3), "│       logout");
        assert_eq!(line(&buf, 4), "│   ▸ fix");
        assert_eq!(line(&buf, 5), "│   * main");
        assert_eq!(line(&buf, 6), "│ ▸ Remotes");

        // The current branch stands out; the selected row uses the selection colours.
        let theme = crate::color::ColorTheme::default();
        assert_eq!(buf[(6, 5)].fg, theme.list_head_fg);
        assert!(buf[(6, 5)].modifier.contains(Modifier::BOLD));
        assert_eq!(buf[(8, 1)].bg, theme.ref_selected_bg);
        assert_eq!(buf[(8, 5)].bg, Color::Reset);
    }
}
