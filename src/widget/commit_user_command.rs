use std::rc::Rc;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Padding, Paragraph, StatefulWidget, Widget},
};

use crate::app::AppContext;

#[derive(Debug, Default)]
pub struct CommitUserCommandState {
    height: usize,
    offset: usize,
    navigation_stops: Vec<usize>,
    line_count: usize,
}

impl CommitUserCommandState {
    pub fn set_navigation_lines(&mut self, lines: &[Line<'_>]) {
        self.navigation_stops = navigation_stops(lines);
        self.line_count = lines.len();
    }

    pub fn jump_next(&mut self) {
        let max_offset = self.line_count.saturating_sub(self.height);
        if let Some(offset) = self
            .navigation_stops
            .iter()
            .copied()
            .map(|offset| offset.min(max_offset))
            .find(|&offset| offset > self.offset)
        {
            self.offset = offset;
        }
    }

    pub fn jump_previous(&mut self) {
        let max_offset = self.line_count.saturating_sub(self.height);
        if let Some(offset) = self
            .navigation_stops
            .iter()
            .rev()
            .copied()
            .map(|offset| offset.min(max_offset))
            .find(|&offset| offset < self.offset)
        {
            self.offset = offset;
        }
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

// Prototype: recognize plain Git headers and delta's usual labels/decorations
// after ANSI conversion. Custom styles that omit these markers are unsupported.
fn navigation_stops(lines: &[Line<'_>]) -> Vec<usize> {
    let text: Vec<String> = lines.iter().map(ToString::to_string).collect();
    let is_rule = |line: &str| line.len() >= 6 && line.chars().all(|c| c == '─');
    let is_box_top = |line: &str| line.strip_suffix('┐').is_some_and(is_rule);
    text.iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim_end();
            let boxed = is_box_top(line);
            let underlined = !line.is_empty()
                && !line.starts_with('│')
                && text
                    .get(index + 1)
                    .is_some_and(|next| is_rule(next.trim_end()));
            let labeled = ["Δ ", "• ", "added: ", "removed: ", "renamed: ", "copied: "]
                .iter()
                .any(|prefix| line.starts_with(prefix));
            // A labeled hunk immediately below its box top is the same stop.
            let below_box = index > 0 && is_box_top(text[index - 1].trim_end());
            (boxed
                || underlined
                || line.starts_with("diff --git ")
                || line.starts_with("@@ ")
                || line.starts_with("@@@ ")
                || (labeled && !below_box))
                .then_some(index)
        })
        .collect()
}

pub struct CommitUserCommand<'a> {
    lines: &'a Vec<Line<'a>>,
    ctx: Rc<AppContext>,
}

impl<'a> CommitUserCommand<'a> {
    pub fn new(lines: &'a Vec<Line<'a>>, ctx: Rc<AppContext>) -> Self {
        Self { lines, ctx }
    }
}

impl StatefulWidget for CommitUserCommand<'_> {
    type State = CommitUserCommandState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let content_area_height = area.height as usize - 1; // minus the top border
        self.update_state(state, self.lines.len(), content_area_height);

        self.render_user_command_lines(area, buf, state);
    }
}

impl CommitUserCommand<'_> {
    fn render_user_command_lines(
        &self,
        area: Rect,
        buf: &mut Buffer,
        state: &mut CommitUserCommandState,
    ) {
        let lines = self
            .lines
            .iter()
            .skip(state.offset)
            .take(area.height as usize - 1)
            .cloned()
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(self.ctx.color_theme.fg))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .style(Style::default().fg(self.ctx.color_theme.divider_fg))
                    .padding(Padding::horizontal(2)),
            );
        paragraph.render(area, buf);
    }

    fn update_state(
        &self,
        state: &mut CommitUserCommandState,
        line_count: usize,
        area_height: usize,
    ) {
        state.height = area_height;
        state.offset = state.offset.min(line_count.saturating_sub(area_height));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<Line<'_>> {
        text.lines().map(Line::from).collect()
    }

    #[test]
    fn detects_plain_git_headers() {
        let output = lines("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n@@@ -1 -1 +1 @@@\n context");
        assert_eq!(navigation_stops(&output), vec![0, 3, 6]);
    }

    #[test]
    fn detects_delta_labels_without_duplicate_boxed_hunks() {
        let output = lines("\nΔ src/main.rs\n──────────\n\n─────┐\n• 1: │\n─────┘\n│ 1 │code\n\n• 20: function\nmore code");
        assert_eq!(navigation_stops(&output), vec![1, 4, 9]);
    }

    #[test]
    fn detects_default_delta_without_navigation_labels() {
        let output = lines("\nsrc/main.rs\n──────────\n\n────┐\n1:  │\n────┘\n│ 1 │code\n");
        assert_eq!(navigation_stops(&output), vec![1, 4]);
    }

    #[test]
    fn ignores_diff_body_and_arbitrary_output() {
        let output = lines("ordinary output\n+@@ -1 +1 @@\n-Δ fake\n│ added: code\n│ ────┐\n");
        assert!(navigation_stops(&output).is_empty());
    }

    #[test]
    fn navigates_from_manual_scroll_and_clamps_final_stop() {
        let mut state = CommitUserCommandState {
            height: 5,
            offset: 0,
            navigation_stops: vec![0, 10, 18, 19],
            line_count: 20,
        };
        state.jump_previous();
        assert_eq!(state.offset, 0);
        state.jump_next();
        assert_eq!(state.offset, 10);
        state.jump_next();
        assert_eq!(state.offset, 15);
        state.jump_next();
        assert_eq!(state.offset, 15);
        state.jump_previous();
        assert_eq!(state.offset, 10);
        state.scroll_up();
        state.jump_previous();
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn replacing_output_rebuilds_stops_and_empty_output_is_safe() {
        let mut state = CommitUserCommandState::default();
        state.set_navigation_lines(&lines("@@ -1 +1 @@\n-old\n+new"));
        assert_eq!(state.navigation_stops, vec![0]);
        state.set_navigation_lines(&[]);
        state.jump_next();
        state.jump_previous();
        assert!(state.navigation_stops.is_empty());
        assert_eq!(state.offset, 0);
    }
}
