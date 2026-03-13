use pulldown_cmark::{Event as MarkdownEvent, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::theme;

struct MarkdownRenderState {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    style_stack: Vec<Style>,
    list_depth: usize,
    pending_prefix: Option<(String, Style)>,
    quote_depth: usize,
}

impl MarkdownRenderState {
    fn new(base_style: Style) -> Self {
        Self {
            lines: Vec::new(),
            current: Vec::new(),
            style_stack: vec![base_style],
            list_depth: 0,
            pending_prefix: None,
            quote_depth: 0,
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            let _ = self.style_stack.pop();
        }
    }

    fn push_prefix_if_needed(&mut self) {
        if !self.current.is_empty() {
            return;
        }
        if self.quote_depth > 0 {
            self.current.push(Span::styled(
                "│ ".repeat(self.quote_depth),
                patch_style(self.current_style(), theme::markdown_quote_style()),
            ));
        }
        if let Some((prefix, style)) = self.pending_prefix.take() {
            self.current.push(Span::styled(prefix, style));
        }
    }

    fn push_text(&mut self, text: &str, style: Style) {
        let mut segments = text.split('\n').peekable();
        while let Some(segment) = segments.next() {
            if !segment.is_empty() {
                self.push_prefix_if_needed();
                self.current.push(Span::styled(segment.to_string(), style));
            }
            if segments.peek().is_some() {
                self.flush_line(!segment.is_empty());
            }
        }
    }

    fn flush_line(&mut self, allow_empty: bool) {
        if !self.current.is_empty() || allow_empty {
            self.lines.push(Line::from(std::mem::take(&mut self.current)));
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line(false);
        self.lines
    }
}

fn patch_style(base: Style, overlay: Style) -> Style {
    base.patch(overlay)
}

fn heading_prefix(level: HeadingLevel) -> String {
    let count = match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    };
    format!("{} ", "#".repeat(count))
}

enum MarkdownPiece {
    Block(String),
    BlankLines,
}

fn split_markdown_pieces(content: &str) -> Vec<MarkdownPiece> {
    let normalized = content.replace("\r\n", "\n");
    let mut pieces = Vec::new();
    let mut current = Vec::new();
    let mut blank_run = 0usize;
    let mut in_fence = false;

    for raw_line in normalized.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        let trimmed_start = line.trim_start();
        let is_fence = trimmed_start.starts_with("```");
        let is_blank = line.is_empty();

        if is_blank && !in_fence {
            if !current.is_empty() {
                pieces.push(MarkdownPiece::Block(current.join("\n")));
                current.clear();
            }
            blank_run += 1;
            continue;
        }

        if blank_run > 0 && !pieces.is_empty() {
            pieces.push(MarkdownPiece::BlankLines);
            blank_run = 0;
        }

        current.push(line.to_string());
        if is_fence {
            in_fence = !in_fence;
        }
    }

    if !current.is_empty() {
        pieces.push(MarkdownPiece::Block(current.join("\n")));
    }
    pieces
}

fn markdown_block_lines(content: &str, base_style: Style) -> Vec<Line<'static>> {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(content, options);
    let mut state = MarkdownRenderState::new(base_style);

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    state.flush_line(false);
                    let heading_style =
                        patch_style(state.current_style(), theme::markdown_heading_style());
                    state.pending_prefix = Some((heading_prefix(level), heading_style));
                    state.push_style(heading_style);
                }
                Tag::List(_) => {
                    state.flush_line(false);
                    state.list_depth += 1;
                }
                Tag::Item => {
                    state.flush_line(false);
                    let indent = "  ".repeat(state.list_depth.saturating_sub(1));
                    state.pending_prefix = Some((
                        format!("{indent}• "),
                        patch_style(state.current_style(), theme::markdown_bullet_style()),
                    ));
                }
                Tag::BlockQuote(_) => {
                    state.flush_line(false);
                    state.quote_depth += 1;
                }
                Tag::CodeBlock(_) => {
                    state.flush_line(false);
                    state.push_style(patch_style(
                        state.current_style(),
                        theme::markdown_code_block_style(),
                    ));
                }
                Tag::Emphasis => {
                    state.push_style(state.current_style().add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    state.push_style(state.current_style().add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    state.push_style(state.current_style().add_modifier(Modifier::CROSSED_OUT));
                }
                _ => {}
            },
            MarkdownEvent::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item => {
                    state.flush_line(false);
                    if matches!(tag, TagEnd::Heading(_)) {
                        state.pop_style();
                    }
                }
                TagEnd::List(_) => {
                    state.flush_line(false);
                    state.list_depth = state.list_depth.saturating_sub(1);
                }
                TagEnd::BlockQuote(_) => {
                    state.flush_line(false);
                    state.quote_depth = state.quote_depth.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    state.flush_line(false);
                    state.pop_style();
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                    state.pop_style();
                }
                _ => {}
            },
            MarkdownEvent::Text(text) => {
                let style = state.current_style();
                state.push_text(text.as_ref(), style);
            }
            MarkdownEvent::Code(code) => {
                state.push_prefix_if_needed();
                state.current.push(Span::styled(
                    code.to_string(),
                    patch_style(state.current_style(), theme::markdown_inline_code_style()),
                ));
            }
            MarkdownEvent::SoftBreak | MarkdownEvent::HardBreak => {
                state.flush_line(true);
            }
            MarkdownEvent::Rule => {
                state.flush_line(false);
                state.lines.push(Line::from(Span::styled(
                    "────────────".to_string(),
                    patch_style(base_style, theme::separator_style()),
                )));
            }
            MarkdownEvent::TaskListMarker(done) => {
                state.push_prefix_if_needed();
                let marker = if done { "[x] " } else { "[ ] " };
                state.current.push(Span::styled(
                    marker.to_string(),
                    patch_style(state.current_style(), theme::markdown_bullet_style()),
                ));
            }
            MarkdownEvent::Html(html) | MarkdownEvent::InlineHtml(html) => {
                let style = state.current_style();
                state.push_text(html.as_ref(), style);
            }
            MarkdownEvent::FootnoteReference(text) => {
                state.push_prefix_if_needed();
                state.current.push(Span::styled(format!("[{text}]"), state.current_style()));
            }
            MarkdownEvent::InlineMath(text) | MarkdownEvent::DisplayMath(text) => {
                state.push_prefix_if_needed();
                state.current.push(Span::styled(
                    text.to_string(),
                    patch_style(state.current_style(), theme::markdown_inline_code_style()),
                ));
            }
        }
    }

    let lines = state.finish();
    if lines.is_empty() {
        vec![Line::from(Span::styled(content.to_string(), base_style))]
    } else {
        lines
    }
}

pub(crate) fn markdown_lines(content: &str, base_style: Style) -> Vec<Line<'static>> {
    let mut rendered = Vec::new();

    for piece in split_markdown_pieces(content) {
        match piece {
            MarkdownPiece::Block(block) => {
                rendered.extend(markdown_block_lines(&block, base_style))
            }
            MarkdownPiece::BlankLines => {}
        }
    }

    if rendered.is_empty() {
        vec![Line::from(Span::styled(content.to_string(), base_style))]
    } else {
        rendered
    }
}

pub(crate) fn prefixed_markdown_lines(
    content: &str,
    first_prefix: &str,
    rest_prefix: &str,
    style: Style,
) -> Vec<Line<'static>> {
    markdown_lines(content, style)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { first_prefix } else { rest_prefix };
            let mut spans = vec![Span::styled(prefix.to_string(), style)];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

pub(crate) fn inline_markdown_lines(
    label: &str,
    content: &str,
    label_style: Style,
    base_style: Style,
) -> Vec<Line<'static>> {
    let rendered_lines = markdown_lines(content, base_style);
    let mut lines = Vec::new();

    for (index, line) in rendered_lines.into_iter().enumerate() {
        let prefix = if index == 0 { label } else { "" };
        let mut spans = vec![Span::styled(prefix.to_string(), label_style)];
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        vec![Line::from(Span::styled(label.to_string(), label_style))]
    } else {
        lines
    }
}

pub(crate) fn inline_thinking_lines(content: &str) -> Vec<Line<'static>> {
    inline_markdown_lines(
        "Thinking: ",
        content,
        theme::thinking_label_style(),
        theme::thinking_style(),
    )
}

pub(crate) fn padded_plain_line(line: Line<'static>) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    spans.push(Span::raw(" "));
    spans.extend(line.spans);
    spans.push(Span::raw(" "));
    Line::from(spans)
}

pub(crate) fn padded_plain_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    lines.into_iter().map(padded_plain_line).collect()
}

pub(crate) fn padded_message_line(line: Line<'static>, style: Style) -> Line<'static> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    spans.push(Span::styled(" ".to_string(), style));
    spans.extend(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content.to_string(), patch_style(span.style, style))),
    );
    spans.push(Span::styled(" ".to_string(), style));
    Line::from(spans)
}

pub(crate) fn user_message_lines(content: &str) -> Vec<Line<'static>> {
    markdown_lines(content, theme::user_message_style())
        .into_iter()
        .map(|line| padded_message_line(line, theme::user_message_style()))
        .collect()
}
