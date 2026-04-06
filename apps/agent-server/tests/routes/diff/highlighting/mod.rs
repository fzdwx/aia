use super::{DiffTheme, highlight_document_lines, highlight_line};

#[test]
fn highlights_typescript_even_when_path_is_not_accessible() {
    let html = highlight_line(
        "const attempt = attributes?.attempt",
        "trace-history/missing/trace-timeline.ts",
        DiffTheme::Dark,
    );

    assert!(html.contains("color:#f97583"));
    assert!(html.contains("const"));
    assert_ne!(html, "const attempt = attributes?.attempt");
}

#[test]
fn uses_light_theme_string_color_from_github_theme() {
    let html = highlight_line("const value = \"hi\"", "trace-history/file.ts", DiffTheme::Light);

    assert!(html.contains("color:#032f62"));
}

#[test]
fn highlights_multiline_tsx_with_context() {
    let html_lines = highlight_document_lines(
        "function RetrySummaryList({ trace }: { trace: TraceRecord | null }) {\n    const retryEvents = (trace?.events ?? []).filter(\n        (event) => event.name === \"response.retrying\"\n    )\n\n    return <div className=\"space-y-2\">{retryEvents.length}</div>\n}",
        "trace-history/RetrySummaryList.tsx",
        DiffTheme::Dark,
    );

    assert_eq!(html_lines.len(), 7);
    assert!(html_lines[0].contains("color:#f97583"));
    assert!(html_lines.iter().any(|line| line.contains("color:#f97583")));
    let return_line = html_lines
        .iter()
        .find(|line| line.contains("space-y-2"))
        .expect("should preserve JSX return line");
    assert!(return_line.contains("color:#85e89d") || return_line.contains("color:#79b8ff"));
}

#[test]
fn falls_back_to_plain_text_for_unsupported_extensions() {
    let html = highlight_line("literal <value>", "notes.unsupported", DiffTheme::Dark);

    assert_eq!(html, "literal &lt;value&gt;");
}
