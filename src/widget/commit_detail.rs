use std::rc::Rc;

use chrono::{DateTime, FixedOffset};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, StatefulWidget, Widget},
};

use crate::{
    app::AppContext,
    git::{Commit, FileChange, Ref},
};

#[derive(Debug, Default)]
pub struct CommitDetailState {
    height: usize,
    offset: usize,
    selected_file: Option<usize>,
    files_focused: bool,
    scroll_to_file: bool,
}

impl CommitDetailState {
    pub fn new(file_count: usize) -> Self {
        Self {
            selected_file: (file_count > 0).then_some(0),
            ..Self::default()
        }
    }

    pub fn selected_file(&self) -> Option<usize> {
        self.selected_file
    }
    pub fn files_focused(&self) -> bool {
        self.files_focused
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn height(&self) -> usize {
        self.height
    }

    pub fn toggle_files(&mut self) {
        self.files_focused = !self.files_focused;
        self.scroll_to_file = self.files_focused;
    }

    pub fn select_file(&mut self, index: usize, file_count: usize) {
        self.selected_file = (file_count > 0).then_some(index.min(file_count.saturating_sub(1)));
        self.scroll_to_file = true;
    }

    pub fn restore(&mut self, index: usize, file_count: usize, focused: bool, offset: usize) {
        self.select_file(index, file_count);
        self.files_focused = focused;
        self.offset = offset;
        self.scroll_to_file = focused;
    }

    pub fn scroll_down(&mut self) {
        self.offset = self.offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.offset = self.offset.saturating_sub(1);
    }

    pub fn scroll_page_down(&mut self) {
        self.offset = self.offset.saturating_add(self.height);
    }

    pub fn scroll_page_up(&mut self) {
        self.offset = self.offset.saturating_sub(self.height);
    }

    pub fn scroll_half_page_down(&mut self) {
        self.offset = self.offset.saturating_add(self.height / 2);
    }

    pub fn scroll_half_page_up(&mut self) {
        self.offset = self.offset.saturating_sub(self.height / 2);
    }

    pub fn select_first(&mut self) {
        self.offset = 0;
    }

    pub fn select_last(&mut self) {
        self.offset = usize::MAX;
    }
}

pub struct CommitDetail<'a> {
    commit: &'a Commit,
    changes: &'a Vec<FileChange>,
    refs: &'a Vec<Ref>,
    ctx: Rc<AppContext>,
}

impl<'a> CommitDetail<'a> {
    pub fn new(
        commit: &'a Commit,
        changes: &'a Vec<FileChange>,
        refs: &'a Vec<Ref>,
        ctx: Rc<AppContext>,
    ) -> Self {
        Self {
            commit,
            changes,
            refs,
            ctx,
        }
    }
}

impl StatefulWidget for CommitDetail<'_> {
    type State = CommitDetailState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let [labels_area, value_area] =
            Layout::horizontal([Constraint::Length(12), Constraint::Min(0)]).areas(area);

        let (mut label_lines, mut value_lines) = self.contents(area);

        let content_area_height = area.height.saturating_sub(1) as usize;
        let file_start = value_lines.len().saturating_sub(self.changes.len());
        if let Some(index) = state.selected_file {
            if let Some(line) = value_lines.get_mut(file_start + index) {
                let modifier = if state.files_focused {
                    Modifier::REVERSED
                } else {
                    Modifier::BOLD
                };
                *line = line
                    .clone()
                    .patch_style(Style::default().add_modifier(modifier));
            }
            if (state.scroll_to_file || state.files_focused) && content_area_height > 0 {
                let row = file_start + index;
                if row < state.offset {
                    state.offset = row;
                }
                if row >= state.offset.saturating_add(content_area_height) {
                    state.offset = row + 1 - content_area_height;
                }
            }
        }
        state.scroll_to_file = false;
        self.update_state(state, value_lines.len(), content_area_height);

        label_lines = label_lines.into_iter().skip(state.offset).collect();
        value_lines = value_lines.into_iter().skip(state.offset).collect();

        self.render_labels_paragraph(label_lines, labels_area, buf);
        self.render_value_paragraph(value_lines, value_area, buf);
    }
}

impl CommitDetail<'_> {
    fn render_labels_paragraph(&self, lines: Vec<Line>, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(self.ctx.color_theme.fg))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .style(Style::default().fg(self.ctx.color_theme.divider_fg))
                    .padding(Padding::left(2)),
            );
        paragraph.render(area, buf);
    }

    fn render_value_paragraph(&self, lines: Vec<Line>, area: Rect, buf: &mut Buffer) {
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(self.ctx.color_theme.fg))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .style(Style::default().fg(self.ctx.color_theme.divider_fg))
                    .padding(Padding::new(1, 2, 0, 0)),
            );
        paragraph.render(area, buf);
    }

    fn contents(&self, area: Rect) -> (Vec<Line<'_>>, Vec<Line<'_>>) {
        let mut label_lines: Vec<Line> = Vec::new();
        let mut value_lines: Vec<Line> = Vec::new();

        label_lines.push(Line::from("   Author: ").fg(self.ctx.color_theme.detail_label_fg));
        label_lines.push(self.empty_line());
        value_lines.extend(self.author_lines());

        if is_author_committer_different(self.commit) {
            label_lines.push(Line::from("Committer: ").fg(self.ctx.color_theme.detail_label_fg));
            label_lines.push(self.empty_line());
            value_lines.extend(self.committer_lines());
        }

        label_lines.push(Line::from("      SHA: ").fg(self.ctx.color_theme.detail_label_fg));
        value_lines.push(self.sha_line());

        if has_parent(self.commit) {
            label_lines.push(Line::from("  Parents: ").fg(self.ctx.color_theme.detail_label_fg));
            value_lines.push(self.parents_line());
        }

        if has_refs(self.refs) {
            label_lines.push(Line::from("     Refs: ").fg(self.ctx.color_theme.detail_label_fg));
            value_lines.push(self.refs_line());
        }

        value_lines.push(self.divider_line(area.width as usize));
        value_lines.extend(self.commit_message_lines());

        value_lines.push(self.divider_line(area.width as usize));
        value_lines.push(Line::raw(if self.changes.is_empty() {
            "No changed files"
        } else {
            "Files (working tree) · Tab: focus · e: edit"
        }));
        value_lines.extend(self.changes_lines());

        (label_lines, value_lines)
    }

    fn author_lines(&self) -> Vec<Line<'_>> {
        self.author_committer_lines(
            &self.commit.author_name,
            &self.commit.author_email,
            &self.commit.author_date,
        )
    }

    fn committer_lines(&self) -> Vec<Line<'_>> {
        self.author_committer_lines(
            &self.commit.committer_name,
            &self.commit.committer_email,
            &self.commit.committer_date,
        )
    }

    fn author_committer_lines<'a>(
        &'a self,
        name: &'a str,
        email: &'a str,
        date: &'a DateTime<FixedOffset>,
    ) -> Vec<Line<'a>> {
        let date_str = if self.ctx.ui_config.detail.date_local {
            let local = date.with_timezone(&chrono::Local);
            local
                .format(&self.ctx.ui_config.detail.date_format)
                .to_string()
        } else {
            date.format(&self.ctx.ui_config.detail.date_format)
                .to_string()
        };
        vec![
            Line::from(vec![
                name.fg(self.ctx.color_theme.detail_name_fg),
                " <".into(),
                email.fg(self.ctx.color_theme.detail_email_fg),
                "> ".into(),
            ]),
            Line::from(date_str.fg(self.ctx.color_theme.detail_date_fg)),
        ]
    }

    fn sha_line(&self) -> Line<'_> {
        Line::from(
            self.commit
                .commit_hash
                .as_str()
                .fg(self.ctx.color_theme.detail_hash_fg),
        )
    }

    fn parents_line(&self) -> Line<'_> {
        let mut spans = Vec::new();
        let parents = &self.commit.parent_commit_hashes;
        for (i, hash) in parents
            .iter()
            .map(|hash| hash.as_short_hash().fg(self.ctx.color_theme.detail_hash_fg))
            .enumerate()
        {
            spans.push(hash);
            if i < parents.len() - 1 {
                spans.push(Span::raw(" "));
            }
        }
        Line::from(spans)
    }

    fn refs_line(&self) -> Line<'_> {
        let ref_spans = self.refs.iter().filter_map(|r| match r {
            Ref::Branch { name, .. } => Some(
                Span::raw(name)
                    .fg(self.ctx.color_theme.detail_ref_branch_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            Ref::RemoteBranch { name, .. } => Some(
                Span::raw(name)
                    .fg(self.ctx.color_theme.detail_ref_remote_branch_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            Ref::Tag { name, .. } => Some(
                Span::raw(name)
                    .fg(self.ctx.color_theme.detail_ref_tag_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            Ref::Stash { .. } => None,
        });

        let mut spans = Vec::new();
        for (i, ref_span) in ref_spans.enumerate() {
            spans.push(ref_span);
            if i < self.refs.len() - 1 {
                spans.push(Span::raw(" "));
            }
        }
        Line::from(spans)
    }

    fn commit_message_lines(&self) -> Vec<Line<'_>> {
        let subject_line = Line::from(self.commit.subject.as_str().bold());

        let mut lines = vec![subject_line];

        if self.commit.body.is_empty() {
            return lines;
        }

        let body_lines = self.commit.body.lines().map(Line::raw);

        lines.push(self.empty_line());
        lines.extend(body_lines);

        lines
    }

    fn changes_lines(&self) -> Vec<Line<'_>> {
        self.changes
            .iter()
            .map(|c| match c {
                FileChange::Add { path } => Line::from(vec![
                    "A".fg(self.ctx.color_theme.detail_file_change_add_fg),
                    " ".into(),
                    file_label(path).into(),
                ]),
                FileChange::Modify { path } => Line::from(vec![
                    "M".fg(self.ctx.color_theme.detail_file_change_modify_fg),
                    " ".into(),
                    file_label(path).into(),
                ]),
                FileChange::Delete { path } => Line::from(vec![
                    "D".fg(self.ctx.color_theme.detail_file_change_delete_fg),
                    " ".into(),
                    file_label(path).into(),
                ]),
                FileChange::Move { from, to } => Line::from(vec![
                    "R".fg(self.ctx.color_theme.detail_file_change_move_fg),
                    " ".into(),
                    file_label(from).into(),
                    " -> ".into(),
                    file_label(to).into(),
                ]),
            })
            .collect()
    }

    fn empty_line(&self) -> Line<'_> {
        Line::raw("")
    }

    fn divider_line(&self, width: usize) -> Line<'_> {
        Line::from("─".repeat(width).fg(self.ctx.color_theme.divider_fg))
    }

    fn update_state(&self, state: &mut CommitDetailState, line_count: usize, area_height: usize) {
        state.height = area_height;
        state.offset = state.offset.min(line_count.saturating_sub(area_height));
    }
}

fn file_label(path: &str) -> String {
    path.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn is_author_committer_different(commit: &Commit) -> bool {
    commit.author_name != commit.committer_name
        || commit.author_email != commit.committer_email
        || commit.author_date != commit.committer_date
}

fn has_parent(commit: &Commit) -> bool {
    !commit.parent_commit_hashes.is_empty()
}

fn has_refs(refs: &[Ref]) -> bool {
    refs.iter().any(|r| {
        matches!(
            r,
            Ref::Branch { .. } | Ref::RemoteBranch { .. } | Ref::Tag { .. }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{CoreConfig, UiConfig},
        keybind::KeyBind,
        protocol::ImageProtocol,
    };

    #[test]
    fn test_selected_file_stays_visible_after_navigation_and_resize() {
        let ctx = Rc::new(AppContext {
            keybind: KeyBind::new(None),
            core_config: CoreConfig::default(),
            ui_config: UiConfig::default(),
            color_theme: crate::color::ColorTheme::default(),
            image_protocol: ImageProtocol::Iterm2,
        });
        let commit = Commit {
            subject: "subject".into(),
            body: "body\n".repeat(30),
            ..Commit::default()
        };
        let changes = (0..30)
            .map(|i| FileChange::Add {
                path: format!("file-{i:02}.txt"),
            })
            .collect();
        let refs = Vec::new();
        let mut state = CommitDetailState::new(30);
        state.toggle_files();
        state.select_file(29, 30);
        for (width, height) in [(80, 12), (30, 3), (1, 1), (0, 0), (80, 8)] {
            let area = Rect::new(0, 0, width, height);
            let mut buffer = Buffer::empty(area);
            CommitDetail::new(&commit, &changes, &refs, ctx.clone()).render(
                area,
                &mut buffer,
                &mut state,
            );
            if width >= 30 && height >= 3 {
                let text = buffer
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect::<String>();
                assert!(text.contains("file-29.txt"), "{text}");
                assert!(buffer
                    .content
                    .iter()
                    .any(|cell| cell.modifier.contains(Modifier::REVERSED)));
            }
        }
        state.select_file(0, 30);
        assert_eq!(state.selected_file(), Some(0));
        state.select_file(usize::MAX, 30);
        assert_eq!(state.selected_file(), Some(29));
        state.select_file(0, 0);
        assert_eq!(state.selected_file(), None);
        assert_eq!(file_label("file\twith\ncontrols"), "file\\twith\\ncontrols");
    }
}
