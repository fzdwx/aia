use std::sync::LazyLock;

use axum::{Json, http::StatusCode, response::IntoResponse};
use similar::{ChangeTag, TextDiff};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, FontStyle, Style, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};

use super::{DiffHunk, DiffLine, DiffRequest, DiffResponse};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

fn resolve_theme(name: Option<&str>) -> &'static Theme {
    let key = match name {
        Some("light") => "InspiredGitHub",
        Some("dark") | None => "base16-ocean.dark",
        Some(other) => other,
    };
    THEME_SET
        .themes
        .get(key)
        .unwrap_or_else(|| THEME_SET.themes.values().next().unwrap())
}

fn resolve_syntax(file_name: &str) -> &'static SyntaxReference {
    let ss = &*SYNTAX_SET;
    ss.find_syntax_for_file(file_name)
        .ok()
        .flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

fn highlight_line(line: &str, syntax: &SyntaxReference, theme: &Theme) -> String {
    let ss = &*SYNTAX_SET;
    let mut h = HighlightLines::new(syntax, theme);
    let line_with_newline = format!("{}\n", line);
    match h.highlight_line(&line_with_newline, ss) {
        Ok(regions) => styled_regions_to_html(&regions),
        Err(_) => {
            let mut out = String::new();
            html_escape_into(&mut out, line);
            out
        }
    }
}

fn styled_regions_to_html(regions: &[(Style, &str)]) -> String {
    let mut out = String::with_capacity(regions.len() * 40);
    for (style, text) in regions {
        let color = style.foreground;
        let css = format_style_css(color, style.font_style);
        out.push_str("<span style=\"");
        out.push_str(&css);
        out.push_str("\">");
        html_escape_into(&mut out, text);
        out.push_str("</span>");
    }
    out
}

fn format_style_css(color: Color, font_style: FontStyle) -> String {
    let mut css = format!("color:#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    if font_style.contains(FontStyle::BOLD) {
        css.push_str(";font-weight:bold");
    }
    if font_style.contains(FontStyle::ITALIC) {
        css.push_str(";font-style:italic");
    }
    if font_style.contains(FontStyle::UNDERLINE) {
        css.push_str(";text-decoration:underline");
    }
    css
}

fn html_escape_into(buf: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            _ => buf.push(ch),
        }
    }
}

fn compute_contents_diff(
    file_name: &str,
    old_content: &str,
    new_content: &str,
    theme: &Theme,
) -> DiffResponse {
    let syntax = resolve_syntax(file_name);
    let diff = TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .diff_lines(old_content, new_content);

    let mut hunks = Vec::new();

    for group in diff.grouped_ops(3) {
        let mut lines = Vec::new();
        let first = group.first().unwrap();
        let last = group.last().unwrap();

        let old_start = first.old_range().start as u32 + 1;
        let old_count = (last.old_range().end - first.old_range().start) as u32;
        let new_start = first.new_range().start as u32 + 1;
        let new_count = (last.new_range().end - first.new_range().start) as u32;

        for op in &group {
            for change in diff.iter_changes(op) {
                let text = change.value();
                let display_text = text.strip_suffix('\n').unwrap_or(text);
                let html = highlight_line(display_text, syntax, theme);

                match change.tag() {
                    ChangeTag::Equal => {
                        lines.push(DiffLine {
                            kind: "ctx",
                            old_ln: change.old_index().map(|i| i as u32 + 1),
                            new_ln: change.new_index().map(|i| i as u32 + 1),
                            html,
                        });
                    }
                    ChangeTag::Delete => {
                        lines.push(DiffLine {
                            kind: "del",
                            old_ln: change.old_index().map(|i| i as u32 + 1),
                            new_ln: None,
                            html,
                        });
                    }
                    ChangeTag::Insert => {
                        lines.push(DiffLine {
                            kind: "add",
                            old_ln: None,
                            new_ln: change.new_index().map(|i| i as u32 + 1),
                            html,
                        });
                    }
                }
            }
        }

        hunks.push(DiffHunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines,
        });
    }

    DiffResponse { hunks }
}

fn compute_patch_diff(patch: &str, theme: &Theme) -> DiffResponse {
    let mut file_name = "file.txt";
    let mut hunks = Vec::new();
    let mut current_lines: Vec<DiffLine> = Vec::new();
    let mut old_start: u32 = 1;
    let mut new_start: u32 = 1;
    let mut old_count: u32 = 0;
    let mut new_count: u32 = 0;
    let mut in_hunk = false;

    for raw_line in patch.lines() {
        if let Some(rest) = raw_line.strip_prefix("diff --git ") {
            // Extract file name from "a/path b/path"
            if let Some(b_part) = rest.split(' ').nth(1) {
                file_name = b_part.strip_prefix("b/").unwrap_or(b_part);
            }
            continue;
        }

        if raw_line.starts_with("--- ") || raw_line.starts_with("+++ ") {
            // Also try to extract name from +++ header
            if raw_line.starts_with("+++ ") {
                let path = raw_line[4..].trim();
                if path != "/dev/null" {
                    file_name = path.strip_prefix("b/").unwrap_or(path);
                }
            }
            continue;
        }

        if raw_line.starts_with("@@") {
            // Flush previous hunk
            if in_hunk && !current_lines.is_empty() {
                hunks.push(DiffHunk {
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: std::mem::take(&mut current_lines),
                });
            }

            // Parse @@ -old_start,old_count +new_start,new_count @@
            if let Some((os, oc, ns, nc)) = parse_hunk_header(raw_line) {
                old_start = os;
                old_count = oc;
                new_start = ns;
                new_count = nc;
            } else {
                old_start = 1;
                old_count = 0;
                new_start = 1;
                new_count = 0;
            }
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            continue;
        }

        let syntax = resolve_syntax(file_name);

        if let Some(content) = raw_line.strip_prefix('+') {
            let html = highlight_line(content, syntax, theme);
            new_count = new_count.wrapping_add(0); // count tracked by header
            current_lines.push(DiffLine {
                kind: "add",
                old_ln: None,
                new_ln: Some(new_start + count_kind(&current_lines, "add", "ctx")),
                html,
            });
        } else if let Some(content) = raw_line.strip_prefix('-') {
            let html = highlight_line(content, syntax, theme);
            current_lines.push(DiffLine {
                kind: "del",
                old_ln: Some(old_start + count_kind(&current_lines, "del", "ctx")),
                new_ln: None,
                html,
            });
        } else {
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            let html = highlight_line(content, syntax, theme);
            let old_ln = old_start + count_kind(&current_lines, "del", "ctx");
            let new_ln = new_start + count_kind(&current_lines, "add", "ctx");
            current_lines.push(DiffLine {
                kind: "ctx",
                old_ln: Some(old_ln),
                new_ln: Some(new_ln),
                html,
            });
        }
    }

    // Flush last hunk
    if !current_lines.is_empty() {
        hunks.push(DiffHunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines: current_lines,
        });
    }

    DiffResponse { hunks }
}

fn count_kind(lines: &[DiffLine], kind1: &str, kind2: &str) -> u32 {
    lines
        .iter()
        .filter(|l| l.kind == kind1 || l.kind == kind2)
        .count() as u32
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    // @@ -old_start,old_count +new_start,new_count @@
    let line = line.strip_prefix("@@")?;
    let line = line.trim_start();
    let end = line.find("@@")?;
    let range_part = line[..end].trim();

    let mut parts = range_part.split_whitespace();

    let old_part = parts.next()?.strip_prefix('-')?;
    let new_part = parts.next()?.strip_prefix('+')?;

    let (os, oc) = parse_range(old_part);
    let (ns, nc) = parse_range(new_part);

    Some((os, oc, ns, nc))
}

fn parse_range(s: &str) -> (u32, u32) {
    if let Some((start, count)) = s.split_once(',') {
        (
            start.parse().unwrap_or(1),
            count.parse().unwrap_or(0),
        )
    } else {
        (s.parse().unwrap_or(1), 1)
    }
}

pub(crate) async fn compute_diff(
    Json(body): Json<DiffRequest>,
) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || match body {
        DiffRequest::Contents {
            file_name,
            old_content,
            new_content,
            theme,
        } => {
            let theme = resolve_theme(theme.as_deref());
            compute_contents_diff(&file_name, &old_content, &new_content, theme)
        }
        DiffRequest::Patch { patch, theme } => {
            let theme = resolve_theme(theme.as_deref());
            compute_patch_diff(&patch, theme)
        }
    })
    .await;

    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
