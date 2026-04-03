use std::{path::Path, sync::LazyLock};

use tree_sitter::Parser;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, Highlighter, HtmlRenderer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DiffTheme {
    Dark,
    Light,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThemeStyle {
    foreground: String,
    bold: bool,
    italic: bool,
    underline: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThemeRule {
    scopes: Vec<String>,
    style: ThemeStyle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThemePalette {
    default_style: ThemeStyle,
    rules: Vec<ThemeRule>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HighlightLanguage {
    JavaScript,
    Jsx,
    TypeScript,
    Tsx,
    Json,
    Css,
    Html,
    Rust,
}

#[derive(Clone, Copy)]
struct LanguageSupport {
    language: HighlightLanguage,
    config: Option<&'static HighlightConfiguration>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TokenSpan {
    start: usize,
    end: usize,
    capture: &'static str,
}

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "keyword",
    "module",
    "number",
    "operator",
    "property",
    "property.builtin",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "string.special.key",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

static JAVASCRIPT_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(
        tree_sitter_javascript::LANGUAGE.into(),
        "javascript",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::INJECTIONS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    )
});

static JSX_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    let highlights = format!(
        "{}\n{}",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
    );
    build_config(
        tree_sitter_javascript::LANGUAGE.into(),
        "jsx",
        &highlights,
        tree_sitter_javascript::INJECTIONS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    )
});

static TYPESCRIPT_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "typescript",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        "",
        tree_sitter_typescript::LOCALS_QUERY,
    )
});

static TSX_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    let highlights = format!(
        "{}\n{}",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
    );
    build_config(
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        "tsx",
        &highlights,
        "",
        tree_sitter_typescript::LOCALS_QUERY,
    )
});

static JSON_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(
        tree_sitter_json::LANGUAGE.into(),
        "json",
        tree_sitter_json::HIGHLIGHTS_QUERY,
        "",
        "",
    )
});

static CSS_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(tree_sitter_css::LANGUAGE.into(), "css", tree_sitter_css::HIGHLIGHTS_QUERY, "", "")
});

static HTML_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(
        tree_sitter_html::LANGUAGE.into(),
        "html",
        tree_sitter_html::HIGHLIGHTS_QUERY,
        tree_sitter_html::INJECTIONS_QUERY,
        "",
    )
});

static RUST_CONFIG: LazyLock<Option<HighlightConfiguration>> = LazyLock::new(|| {
    build_config(
        tree_sitter_rust::LANGUAGE.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        tree_sitter_rust::INJECTIONS_QUERY,
        "",
    )
});

static DARK_THEME_PALETTE: LazyLock<ThemePalette> =
    LazyLock::new(|| parse_theme_palette(include_str!("../../../themes/github-dark.tmTheme")));

static LIGHT_THEME_PALETTE: LazyLock<ThemePalette> =
    LazyLock::new(|| parse_theme_palette(include_str!("../../../themes/github-light.tmTheme")));

fn build_config(
    language: tree_sitter::Language,
    name: &str,
    highlights_query: &str,
    injections_query: &str,
    locals_query: &str,
) -> Option<HighlightConfiguration> {
    let mut config = HighlightConfiguration::new(
        language,
        name,
        highlights_query,
        injections_query,
        locals_query,
    )
    .ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(config)
}

pub(super) fn highlight_line(line: &str, file_name: &str, theme: DiffTheme) -> String {
    highlight_document_lines(line, file_name, theme)
        .into_iter()
        .next()
        .unwrap_or_else(|| escape_html(line))
}

pub(super) fn highlight_document_lines(
    content: &str,
    file_name: &str,
    theme: DiffTheme,
) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    let Some(language_support) = resolve_language_support(file_name) else {
        return plain_html_lines(content);
    };

    if let Some(config) = language_support.config {
        let mut highlighter = Highlighter::new();
        let mut renderer = HtmlRenderer::new();
        let mut source = content.to_owned();
        let had_trailing_newline = source.ends_with('\n');
        if !had_trailing_newline {
            source.push('\n');
        }
        let injection_callback = |language_name: &str| -> Option<&HighlightConfiguration> {
            injection_config(language_name)
        };

        let highlights =
            match highlighter.highlight(config, source.as_bytes(), None, injection_callback) {
                Ok(highlights) => highlights,
                Err(_) => return parser_fallback_lines(content, language_support.language, theme),
            };

        if renderer
            .render(highlights, source.as_bytes(), &|highlight, html| {
                write_style_attributes(theme, highlight, html)
            })
            .is_err()
        {
            return parser_fallback_lines(content, language_support.language, theme);
        }

        let mut lines: Vec<String> = renderer.lines().map(str::to_owned).collect();
        if !had_trailing_newline && lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        if !lines.is_empty() && lines.iter().any(|line| line.contains("<span ")) {
            return lines;
        }
    }

    parser_fallback_lines(content, language_support.language, theme)
}

fn resolve_language_support(file_name: &str) -> Option<LanguageSupport> {
    let path = Path::new(file_name);
    let extension = path.extension().and_then(|ext| ext.to_str())?.to_ascii_lowercase();
    language_support_for_extension(&extension)
}

fn language_support_for_extension(extension: &str) -> Option<LanguageSupport> {
    match extension {
        "js" | "mjs" | "cjs" => Some(LanguageSupport {
            language: HighlightLanguage::JavaScript,
            config: JAVASCRIPT_CONFIG.as_ref(),
        }),
        "jsx" => {
            Some(LanguageSupport { language: HighlightLanguage::Jsx, config: JSX_CONFIG.as_ref() })
        }
        "ts" | "mts" | "cts" => Some(LanguageSupport {
            language: HighlightLanguage::TypeScript,
            config: TYPESCRIPT_CONFIG.as_ref(),
        }),
        "tsx" => {
            Some(LanguageSupport { language: HighlightLanguage::Tsx, config: TSX_CONFIG.as_ref() })
        }
        "json" | "jsonc" => Some(LanguageSupport {
            language: HighlightLanguage::Json,
            config: JSON_CONFIG.as_ref(),
        }),
        "css" => {
            Some(LanguageSupport { language: HighlightLanguage::Css, config: CSS_CONFIG.as_ref() })
        }
        "html" | "htm" => Some(LanguageSupport {
            language: HighlightLanguage::Html,
            config: HTML_CONFIG.as_ref(),
        }),
        "rs" => Some(LanguageSupport {
            language: HighlightLanguage::Rust,
            config: RUST_CONFIG.as_ref(),
        }),
        _ => None,
    }
}

fn parser_fallback_lines(
    content: &str,
    language: HighlightLanguage,
    theme: DiffTheme,
) -> Vec<String> {
    let Some(mut parser) = parser_for_language(language) else {
        return plain_html_lines(content);
    };
    let Some(tree) = parser.parse(content, None) else {
        return plain_html_lines(content);
    };

    let mut spans = Vec::new();
    collect_token_spans(tree.root_node(), content.as_bytes(), &mut spans);
    if spans.is_empty() {
        return plain_html_lines(content);
    }

    render_lines_from_spans(content, &spans, theme)
}

fn parser_for_language(language: HighlightLanguage) -> Option<Parser> {
    let mut parser = Parser::new();
    let target = match language {
        HighlightLanguage::JavaScript | HighlightLanguage::Jsx => {
            tree_sitter_javascript::LANGUAGE.into()
        }
        HighlightLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        HighlightLanguage::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        HighlightLanguage::Json => tree_sitter_json::LANGUAGE.into(),
        HighlightLanguage::Css => tree_sitter_css::LANGUAGE.into(),
        HighlightLanguage::Html => tree_sitter_html::LANGUAGE.into(),
        HighlightLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
    };
    parser.set_language(&target).ok()?;
    Some(parser)
}

fn collect_token_spans(node: tree_sitter::Node<'_>, source: &[u8], spans: &mut Vec<TokenSpan>) {
    if node.byte_range().is_empty() {
        return;
    }

    if node.child_count() == 0 {
        if let Some(capture) = classify_leaf_capture(node, source) {
            spans.push(TokenSpan { start: node.start_byte(), end: node.end_byte(), capture });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_token_spans(child, source, spans);
    }
}

fn classify_leaf_capture(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<&'static str> {
    if is_comment_node(node) {
        return Some("comment");
    }
    if is_string_node(node) {
        return Some(if is_property_key(node) { "string.special.key" } else { "string" });
    }
    if is_number_node(node) {
        return Some("number");
    }
    if is_tag_node(node) {
        return Some("tag");
    }
    if is_attribute_node(node) {
        return Some("attribute");
    }
    if is_type_node(node) {
        return Some("type");
    }
    if is_property_node(node) {
        return Some("property");
    }
    if is_parameter_node(node) {
        return Some("variable.parameter");
    }
    if is_keyword_node(source, node.byte_range()) {
        return Some("keyword");
    }
    None
}

fn is_comment_node(node: tree_sitter::Node<'_>) -> bool {
    node.kind() == "comment" || node.parent().is_some_and(|parent| parent.kind() == "comment")
}

fn is_string_node(node: tree_sitter::Node<'_>) -> bool {
    node.kind().contains("string")
        || node.parent().is_some_and(|parent| parent.kind().contains("string"))
}

fn is_number_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(node.kind(), "number" | "number_literal" | "integer" | "float")
}

fn is_tag_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(
        node.parent().map(|parent| parent.kind()),
        Some("jsx_opening_element" | "jsx_closing_element" | "jsx_self_closing_element")
    )
}

fn is_attribute_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(node.parent().map(|parent| parent.kind()), Some("jsx_attribute"))
}

fn is_type_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(node.kind(), "type_identifier" | "predefined_type")
        || node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "type_annotation"
                    | "interface_declaration"
                    | "extends_type_clause"
                    | "type_arguments"
            )
        })
}

fn is_property_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(node.kind(), "property_identifier" | "shorthand_property_identifier")
        || (node.kind() == "identifier"
            && matches!(
                node.parent().map(|parent| parent.kind()),
                Some("member_expression" | "pair" | "object_pattern")
            ))
}

fn is_property_key(node: tree_sitter::Node<'_>) -> bool {
    node.parent().is_some_and(|parent| parent.kind() == "pair")
}

fn is_parameter_node(node: tree_sitter::Node<'_>) -> bool {
    node.kind() == "identifier"
        && node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "required_parameter" | "optional_parameter" | "formal_parameters"
            )
        })
}

fn is_keyword_node(source: &[u8], range: std::ops::Range<usize>) -> bool {
    let Ok(text) = std::str::from_utf8(&source[range]) else {
        return false;
    };
    matches!(
        text,
        "function"
            | "const"
            | "let"
            | "var"
            | "return"
            | "switch"
            | "case"
            | "if"
            | "else"
            | "typeof"
            | "null"
            | "undefined"
            | "true"
            | "false"
            | "new"
            | "class"
            | "extends"
            | "export"
            | "import"
            | "from"
            | "as"
            | "try"
            | "catch"
            | "finally"
            | "throw"
            | "await"
            | "async"
    )
}

fn render_lines_from_spans(content: &str, spans: &[TokenSpan], theme: DiffTheme) -> Vec<String> {
    let mut spans = spans.to_vec();
    spans.sort_by_key(|span| (span.start, span.end));

    let mut lines = Vec::new();
    let mut line_start = 0usize;
    for (index, ch) in content.char_indices() {
        if ch == '\n' {
            lines.push(render_line_segment(content, &spans, line_start, index, theme));
            line_start = index + ch.len_utf8();
        }
    }
    if line_start <= content.len() {
        lines.push(render_line_segment(content, &spans, line_start, content.len(), theme));
    }
    lines
}

fn render_line_segment(
    content: &str,
    spans: &[TokenSpan],
    line_start: usize,
    line_end: usize,
    theme: DiffTheme,
) -> String {
    let mut html = String::new();
    let mut cursor = line_start;

    for span in spans {
        if span.end <= line_start || span.start >= line_end {
            continue;
        }
        let start = span.start.max(line_start);
        let end = span.end.min(line_end);
        if cursor < start {
            html_escape_into(&mut html, &content[cursor..start]);
        }
        if start < end {
            html.push_str("<span style=\"");
            html.push_str(&style_for_capture(theme, span.capture));
            html.push_str("\">");
            html_escape_into(&mut html, &content[start..end]);
            html.push_str("</span>");
            cursor = end;
        }
    }

    if cursor < line_end {
        html_escape_into(&mut html, &content[cursor..line_end]);
    }

    html
}

fn injection_config(language_name: &str) -> Option<&'static HighlightConfiguration> {
    match language_name {
        "javascript" => JAVASCRIPT_CONFIG.as_ref(),
        "jsx" => JSX_CONFIG.as_ref(),
        "typescript" => TYPESCRIPT_CONFIG.as_ref(),
        "tsx" => TSX_CONFIG.as_ref(),
        "css" => CSS_CONFIG.as_ref(),
        "html" => HTML_CONFIG.as_ref(),
        "rust" => RUST_CONFIG.as_ref(),
        "json" => JSON_CONFIG.as_ref(),
        _ => None,
    }
}

fn write_style_attributes(theme: DiffTheme, highlight: Highlight, html: &mut Vec<u8>) {
    let name = HIGHLIGHT_NAMES.get(highlight.0).copied().unwrap_or("variable");
    html.extend_from_slice(b"style=\"");
    html.extend_from_slice(style_for_capture(theme, name).as_bytes());
    html.extend_from_slice(b"\"");
}

fn style_for_capture(theme: DiffTheme, name: &str) -> String {
    let palette = match theme {
        DiffTheme::Dark => &*DARK_THEME_PALETTE,
        DiffTheme::Light => &*LIGHT_THEME_PALETTE,
    };
    let style = palette.resolve(capture_scope_candidates(name));

    let mut css = format!("color:{}", style.foreground);
    if style.bold {
        css.push_str(";font-weight:600");
    }
    if style.italic {
        css.push_str(";font-style:italic");
    }
    if style.underline {
        css.push_str(";text-decoration:underline");
    }
    css
}

impl ThemePalette {
    fn resolve(&self, candidates: &[&str]) -> &ThemeStyle {
        for candidate in candidates {
            if let Some(rule) =
                self.rules.iter().find(|rule| rule.scopes.iter().any(|scope| scope == candidate))
            {
                return &rule.style;
            }
        }

        &self.default_style
    }
}

fn capture_scope_candidates(name: &str) -> &'static [&'static str] {
    match name {
        "comment" => &["comment"],
        "constant" | "number" | "constant.builtin" => &["constant", "support.constant"],
        "constructor" | "function" | "function.builtin" => &["entity.name", "entity", "support"],
        "keyword" => &["keyword", "storage", "storage.type"],
        "module" => &["meta.module-reference", "entity.name", "entity"],
        "operator" => &["keyword.operator", "keyword"],
        "property" | "property.builtin" | "string.special.key" => {
            &["meta.property-name", "support.variable"]
        }
        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special" => {
            &["punctuation.definition.string", "punctuation.definition.comment"]
        }
        "string" | "string.special" => &["string", "string.regexp", "source.regexp"],
        "tag" => &["entity.name.tag"],
        "type" | "type.builtin" => &["storage.type", "entity.name", "entity"],
        "attribute" => &["entity.other.attribute-name", "support"],
        "variable.parameter" => &["variable.parameter.function"],
        "variable" | "variable.builtin" | "embedded" => &["variable.other", "variable"],
        _ => &["variable.other", "variable"],
    }
}

fn parse_theme_palette(source: &str) -> ThemePalette {
    let mut rules = Vec::new();
    let mut default_foreground = None;
    let mut current_scope: Option<String> = None;
    let mut current_foreground: Option<String> = None;
    let mut current_font_style: Option<String> = None;
    let mut depth = 0_u32;
    let mut lines = source.lines().map(str::trim);

    while let Some(line) = lines.next() {
        match line {
            "<dict>" => {
                depth += 1;
                if depth == 2 {
                    current_scope = None;
                    current_foreground = None;
                    current_font_style = None;
                }
            }
            "</dict>" => {
                if depth == 2 {
                    if let Some(scope) = current_scope.take() {
                        if let Some(style) =
                            build_theme_style(current_foreground.take(), current_font_style.take())
                        {
                            rules.push(ThemeRule {
                                scopes: scope
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|entry| !entry.is_empty())
                                    .map(ToOwned::to_owned)
                                    .collect(),
                                style,
                            });
                        }
                    } else if default_foreground.is_none() {
                        default_foreground = current_foreground.take();
                    }
                }
                depth = depth.saturating_sub(1);
            }
            "<key>scope</key>" if depth >= 2 => {
                current_scope = lines.next().and_then(parse_string_line).map(str::to_owned);
            }
            "<key>foreground</key>" if depth >= 2 => {
                current_foreground = lines.next().and_then(parse_string_line).map(str::to_owned);
            }
            "<key>fontStyle</key>" if depth >= 2 => {
                current_font_style = lines.next().and_then(parse_string_line).map(str::to_owned);
            }
            _ => {}
        }
    }

    ThemePalette {
        default_style: ThemeStyle {
            foreground: default_foreground.unwrap_or_else(|| "#24292e".to_owned()),
            bold: false,
            italic: false,
            underline: false,
        },
        rules,
    }
}

fn build_theme_style(foreground: Option<String>, font_style: Option<String>) -> Option<ThemeStyle> {
    let foreground = foreground?;
    let font_style = font_style.unwrap_or_default();
    Some(ThemeStyle {
        foreground,
        bold: font_style.split_whitespace().any(|token| token == "bold"),
        italic: font_style.split_whitespace().any(|token| token == "italic"),
        underline: font_style.split_whitespace().any(|token| token == "underline"),
    })
}

fn parse_string_line(line: &str) -> Option<&str> {
    let content = line.strip_prefix("<string>")?;
    content.strip_suffix("</string>")
}

fn plain_html_lines(content: &str) -> Vec<String> {
    content.lines().map(escape_html).collect()
}

fn html_escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
#[path = "../../../tests/routes/diff/highlighting/mod.rs"]
mod tests;
