use std::sync::LazyLock;

use axum::{Json, http::StatusCode, response::IntoResponse};
use similar::{ChangeTag, TextDiff};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, FontStyle, Style, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};

use super::{DiffHunk, DiffLine, DiffRequest, DiffResponse, SplitCell, SplitPair};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

static GITHUB_DARK: LazyLock<Theme> = LazyLock::new(|| {
    let bytes = include_bytes!("../../../themes/github-dark.tmTheme");
    let cursor = std::io::Cursor::new(bytes);
    ThemeSet::load_from_reader(&mut std::io::BufReader::new(cursor))
        .expect("failed to load github-dark theme")
});

static GITHUB_LIGHT: LazyLock<Theme> = LazyLock::new(|| {
    let bytes = include_bytes!("../../../themes/github-light.tmTheme");
    let cursor = std::io::Cursor::new(bytes);
    ThemeSet::load_from_reader(&mut std::io::BufReader::new(cursor))
        .expect("failed to load github-light theme")
});

fn resolve_theme(name: Option<&str>) -> &'static Theme {
    match name {
        Some("light") => &GITHUB_LIGHT,
        _ => &GITHUB_DARK,
    }
}

fn resolve_syntax(file_name: &str) -> &'static SyntaxReference {
    let ss = &*SYNTAX_SET;
    ss.find_syntax_for_file(file_name).ok().flatten().unwrap_or_else(|| ss.find_syntax_plain_text())
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

// ── Split pairs ──────────────────────────────────────────────────

fn build_split_pairs(lines: &[DiffLine]) -> Vec<SplitPair> {
    let mut pairs = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].kind == "ctx" {
            pairs.push(SplitPair {
                left: Some(SplitCell {
                    kind: "ctx",
                    ln: lines[i].old_ln,
                    html: lines[i].html.clone(),
                }),
                right: Some(SplitCell {
                    kind: "ctx",
                    ln: lines[i].new_ln,
                    html: lines[i].html.clone(),
                }),
            });
            i += 1;
            continue;
        }

        let mut dels: Vec<usize> = Vec::new();
        let mut adds: Vec<usize> = Vec::new();
        while i < lines.len() && lines[i].kind == "del" {
            dels.push(i);
            i += 1;
        }
        while i < lines.len() && lines[i].kind == "add" {
            adds.push(i);
            i += 1;
        }

        let max_len = dels.len().max(adds.len());
        for j in 0..max_len {
            let left = dels.get(j).map(|&idx| SplitCell {
                kind: "del",
                ln: lines[idx].old_ln,
                html: lines[idx].html.clone(),
            });
            let right = adds.get(j).map(|&idx| SplitCell {
                kind: "add",
                ln: lines[idx].new_ln,
                html: lines[idx].html.clone(),
            });
            pairs.push(SplitPair { left, right });
        }
    }

    pairs
}

/// Fast path for new files: skip diff, just highlight all lines as "add".
fn compute_all_add(
    file_name: &str,
    content: &str,
    theme: &Theme,
    want_split: bool,
) -> DiffResponse {
    let syntax = resolve_syntax(file_name);
    let raw_lines: Vec<&str> = content.lines().collect();
    let count = raw_lines.len() as u32;

    let lines: Vec<DiffLine> = raw_lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let html = highlight_line(text, syntax, theme);
            DiffLine { kind: "add", old_ln: None, new_ln: Some(i as u32 + 1), html }
        })
        .collect();

    let split_pairs = if want_split {
        lines
            .iter()
            .map(|line| SplitPair {
                left: None,
                right: Some(SplitCell { kind: "add", ln: line.new_ln, html: line.html.clone() }),
            })
            .collect()
    } else {
        Vec::new()
    };

    DiffResponse {
        hunks: vec![DiffHunk {
            old_start: 0,
            old_count: 0,
            new_start: 1,
            new_count: count,
            lines,
            split_pairs,
        }],
        added: count,
        removed: 0,
    }
}

// ── Contents diff ────────────────────────────────────────────────

fn compute_contents_diff(
    file_name: &str,
    old_content: &str,
    new_content: &str,
    theme: &Theme,
    want_split: bool,
) -> DiffResponse {
    // Fast path: old_content is empty → all lines are additions, skip diff
    if old_content.is_empty() && !new_content.is_empty() {
        return compute_all_add(file_name, new_content, theme, want_split);
    }

    let syntax = resolve_syntax(file_name);
    let diff = TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .diff_lines(old_content, new_content);

    let mut hunks = Vec::new();
    let mut total_added: u32 = 0;
    let mut total_removed: u32 = 0;

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
                        total_removed += 1;
                        lines.push(DiffLine {
                            kind: "del",
                            old_ln: change.old_index().map(|i| i as u32 + 1),
                            new_ln: None,
                            html,
                        });
                    }
                    ChangeTag::Insert => {
                        total_added += 1;
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

        let split_pairs = if want_split { build_split_pairs(&lines) } else { Vec::new() };

        hunks.push(DiffHunk { old_start, old_count, new_start, new_count, lines, split_pairs });
    }

    DiffResponse { hunks, added: total_added, removed: total_removed }
}

// ── Patch diff ───────────────────────────────────────────────────

/// Intermediate line before highlighting.
struct RawLine {
    kind: &'static str,
    content: String,
}

/// Dedup consecutive del/add pairs where the raw content is identical → ctx.
fn dedup_identical_changes(lines: Vec<RawLine>) -> Vec<RawLine> {
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if lines[i].kind != "del" {
            result.push(RawLine {
                kind: lines[i].kind,
                content: std::mem::take(&mut { lines[i].content.clone() }),
            });
            i += 1;
            continue;
        }

        // Collect consecutive dels
        let del_start = i;
        while i < lines.len() && lines[i].kind == "del" {
            i += 1;
        }
        let del_end = i;

        // Collect consecutive adds
        let add_start = i;
        while i < lines.len() && lines[i].kind == "add" {
            i += 1;
        }
        let add_end = i;

        let del_count = del_end - del_start;
        let add_count = add_end - add_start;
        let paired = del_count.min(add_count);

        for j in 0..paired {
            let d = &lines[del_start + j];
            let a = &lines[add_start + j];
            if d.content == a.content {
                result.push(RawLine { kind: "ctx", content: d.content.clone() });
            } else {
                result.push(RawLine { kind: "del", content: d.content.clone() });
                result.push(RawLine { kind: "add", content: a.content.clone() });
            }
        }

        // Remaining unpaired dels
        for j in paired..del_count {
            result.push(RawLine { kind: "del", content: lines[del_start + j].content.clone() });
        }
        // Remaining unpaired adds
        for j in paired..add_count {
            result.push(RawLine { kind: "add", content: lines[add_start + j].content.clone() });
        }
    }

    result
}

/// Parsed hunk before highlighting.
struct RawHunk {
    file_name: String,
    old_start: u32,
    new_start: u32,
    lines: Vec<RawLine>,
}

fn compute_patch_diff(patch: &str, theme: &Theme) -> DiffResponse {
    // First pass: parse patch into raw hunks
    let raw_hunks = parse_patch_into_raw_hunks(patch);

    let mut hunks = Vec::new();
    let mut total_added: u32 = 0;
    let mut total_removed: u32 = 0;

    for raw_hunk in raw_hunks {
        // Dedup identical del/add pairs into ctx
        let processed = dedup_identical_changes(raw_hunk.lines);

        let syntax = resolve_syntax(&raw_hunk.file_name);
        let mut lines = Vec::new();
        let mut old_ln = raw_hunk.old_start;
        let mut new_ln = raw_hunk.new_start;

        for raw in &processed {
            let html = highlight_line(&raw.content, syntax, theme);
            match raw.kind {
                "ctx" => {
                    lines.push(DiffLine {
                        kind: "ctx",
                        old_ln: Some(old_ln),
                        new_ln: Some(new_ln),
                        html,
                    });
                    old_ln += 1;
                    new_ln += 1;
                }
                "del" => {
                    total_removed += 1;
                    lines.push(DiffLine { kind: "del", old_ln: Some(old_ln), new_ln: None, html });
                    old_ln += 1;
                }
                "add" => {
                    total_added += 1;
                    lines.push(DiffLine { kind: "add", old_ln: None, new_ln: Some(new_ln), html });
                    new_ln += 1;
                }
                _ => {}
            }
        }

        // Recalculate counts after dedup
        let old_count = lines.iter().filter(|l| l.kind == "ctx" || l.kind == "del").count() as u32;
        let new_count = lines.iter().filter(|l| l.kind == "ctx" || l.kind == "add").count() as u32;

        // Patch mode is always unified, no split_pairs
        hunks.push(DiffHunk {
            old_start: raw_hunk.old_start,
            old_count,
            new_start: raw_hunk.new_start,
            new_count,
            lines,
            split_pairs: Vec::new(),
        });
    }

    DiffResponse { hunks, added: total_added, removed: total_removed }
}

fn parse_patch_into_raw_hunks(patch: &str) -> Vec<RawHunk> {
    let mut file_name = "file.txt".to_string();
    let mut hunks = Vec::new();
    let mut current_lines: Vec<RawLine> = Vec::new();
    let mut old_start: u32 = 1;
    let mut new_start: u32 = 1;
    let mut in_hunk = false;

    for raw_line in patch.lines() {
        if let Some(rest) = raw_line.strip_prefix("diff --git ") {
            if let Some(b_part) = rest.split(' ').nth(1) {
                file_name = b_part.strip_prefix("b/").unwrap_or(b_part).to_string();
            }
            continue;
        }

        if raw_line.starts_with("--- ") || raw_line.starts_with("+++ ") {
            if raw_line.starts_with("+++ ") {
                let path = raw_line[4..].trim();
                if path != "/dev/null" {
                    file_name = path.strip_prefix("b/").unwrap_or(path).to_string();
                }
            }
            continue;
        }

        if raw_line.starts_with("@@") {
            // Flush previous hunk
            if in_hunk && !current_lines.is_empty() {
                hunks.push(RawHunk {
                    file_name: file_name.clone(),
                    old_start,
                    new_start,
                    lines: std::mem::take(&mut current_lines),
                });
            }

            if let Some((os, _, ns, _)) = parse_hunk_header(raw_line) {
                old_start = os;
                new_start = ns;
            } else {
                old_start = 1;
                new_start = 1;
            }
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            continue;
        }

        if let Some(content) = raw_line.strip_prefix('+') {
            current_lines.push(RawLine { kind: "add", content: content.to_string() });
        } else if let Some(content) = raw_line.strip_prefix('-') {
            current_lines.push(RawLine { kind: "del", content: content.to_string() });
        } else {
            let content = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            current_lines.push(RawLine { kind: "ctx", content: content.to_string() });
        }
    }

    // Flush last hunk
    if !current_lines.is_empty() {
        hunks.push(RawHunk { file_name, old_start, new_start, lines: current_lines });
    }

    hunks
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
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
        (start.parse().unwrap_or(1), count.parse().unwrap_or(0))
    } else {
        (s.parse().unwrap_or(1), 1)
    }
}

pub(crate) async fn compute_diff(Json(body): Json<DiffRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || match body {
        DiffRequest::Contents { file_name, old_content, new_content, theme, style } => {
            let theme = resolve_theme(theme.as_deref());
            let want_split = style.as_deref() == Some("split");
            compute_contents_diff(&file_name, &old_content, &new_content, theme, want_split)
        }
        DiffRequest::Patch { patch, theme } => {
            let theme = resolve_theme(theme.as_deref());
            compute_patch_diff(&patch, theme)
        }
    })
    .await;

    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
                .into_response()
        }
    }
}
