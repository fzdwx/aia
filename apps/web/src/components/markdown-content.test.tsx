import { renderToStaticMarkup } from "react-dom/server"
import { readFileSync } from "node:fs"
import { describe, expect, test } from "vite-plus/test"

import { MarkdownRenderer } from "@/components/markdown-content-rich"
import { ThemeProvider } from "@/components/theme-provider"

const WEB_INDEX_CSS = new URL("../index.css", import.meta.url)

function loadMarkdownCss() {
  return readFileSync(WEB_INDEX_CSS, "utf8").replace(/\s+/g, " ")
}

describe("MarkdownRenderer", () => {
  test("renders basic markdown structure", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer
          content={"# Title\n\nParagraph with `code` and **bold**."}
        />
      </ThemeProvider>
    )

    expect(html).toContain("Title")
    expect(html).toContain("Paragraph with")
    expect(html).toContain("code")
    expect(html).toContain("<strong")
  })

  test("passes dark theme state into markstream renderer", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer content={"```ts\nconst answer = 42\n```"} />
      </ThemeProvider>
    )

    expect(html).toContain("is-dark")
  })

  test("renders custom code block header with language and copy button", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer content={"```javascript\nconsole.log('hi')\n```"} />
      </ThemeProvider>
    )

    expect(html).toContain("chat-code-block-header")
    expect(html).toContain("JavaScript")
    expect(html).toContain("Copy")
    expect(html).toContain('aria-label="Copy code"')
  })

  test("renders diagram markdown blocks inside dedicated chat diagram container", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer
          content={"```mermaid\ngraph TD\nA[Start] --> B[End]\n```"}
        />
      </ThemeProvider>
    )

    expect(html).toContain("chat-diagram-block")
    expect(html).toContain('data-diagram-kind="mermaid"')
    expect(html).toContain("chat-diagram-block-label")
  })

  test("preserves multiline code blocks in pre fallback mode", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <div className="markdown-content">
          <pre className="language-javascript">
            <code>{"const a = 1;\nconst b = 2;\nconsole.log(a + b);"}</code>
          </pre>
        </div>
      </ThemeProvider>
    )

    expect(html).toContain("const a = 1;\nconst b = 2;\nconsole.log(a + b);")
    expect(html).toContain("language-javascript")
  })

  test("renders external links with safe target attributes", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer content={"[docs](https://example.com/docs)"} />
      </ThemeProvider>
    )

    expect(html).toContain('href="https://example.com/docs"')
    expect(html).toContain('target="_blank"')
    expect(html).toContain('rel="noreferrer"')
  })

  test("renders ordered and unordered lists with nested structure", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer
          content={
            "1. First item\n2. Second item\n   - Nested bullet\n   - Another nested bullet\n\n- Loose bullet\n- Final bullet"
          }
        />
      </ThemeProvider>
    )

    expect(html).toContain("<ol")
    expect(html).toContain("<ul")
    expect(html).toContain("list-node")
    expect(html).toContain("list-item")
    expect(html).toContain("Nested bullet")
  })

  test("renders task list and complex list item structures", () => {
    const html = renderToStaticMarkup(
      <ThemeProvider defaultTheme="dark">
        <MarkdownRenderer
          content={
            "- [x] done item\n- [ ] pending item\n  > quoted context\n\n  ```ts\n  const answer = 42\n  ```"
          }
        />
      </ThemeProvider>
    )

    expect(html).toContain('type="checkbox"')
    expect(html).toContain("checked")
    expect(html).toContain("pending item")
    expect(html).toContain("chat-code-block")
    expect(html).toContain("blockquote-node")
  })

  test("keeps markdown typography contract for dense chat reading", () => {
    const css = loadMarkdownCss()

    expect(css).toContain(".markdown-content .markstream-react")
    expect(css).toContain("line-height: 1.7")
    expect(css).toContain(".markdown-content .heading-1")
    expect(css).toContain(".markdown-content .heading-2")
    expect(css).toContain(".markdown-content .heading-3")
    expect(css).toContain("font-size: 1.0625rem")
    expect(css).toContain("font-size: 1rem")
    expect(css).toContain("font-size: 0.9375rem")
    expect(css).toContain(".markdown-content .list-node")
    expect(css).toContain(".markdown-content ol.list-node")
    expect(css).toContain(".markdown-content ul.list-node")
    expect(css).toContain("padding-left: 1.25rem")
    expect(css).toContain(".markdown-content .list-item")
    expect(css).toContain(".markdown-content ol.list-node > .list-item::marker")
    expect(css).toContain(".markdown-content ul.list-node > .list-item::marker")
    expect(css).toContain("padding-left: 1.375rem")
    expect(css).toContain('.markdown-content .list-item input[type="checkbox"]')
    expect(css).toContain("accent-color: var(--trace-chart-output)")
    expect(css).toContain(".markdown-content .list-item > .chat-code-block")
    expect(css).toContain("margin-top: 0.5rem")
    expect(css).toContain(".markdown-content .list-item > .blockquote-node")
    expect(css).toContain(".markdown-content .blockquote-node")
    expect(css).toContain("padding: 0.5rem 0.75rem 0.5rem 0.875rem")
    expect(css).toContain("border-radius: 0.5rem")
    expect(css).toContain("line-height: 1.65")
    expect(css).toContain(".markdown-content .inline-code")
    expect(css).toContain("padding: 0.0625rem 0.25rem")
    expect(css).toContain("font-size: 0.84em")
    expect(css).toContain(".markdown-content .chat-diagram-block")
    expect(css).toContain(".markdown-content .chat-diagram-block-label")
    expect(css).toContain('data-diagram-kind="mermaid"')
    expect(css).toContain('data-diagram-kind="d2"')
    expect(css).toContain('data-diagram-kind="infographic"')
    expect(css).toContain(".markdown-content .link-node")
    expect(css).toContain("var(--trace-chart-input)")
    expect(css).toContain(".markdown-content .chat-code-block")
    expect(css).toContain("border: 1px solid")
    expect(css).toContain(".markdown-content .chat-code-block-header")
    expect(css).toContain(
      "background: oklch(from var(--background) l c h / 82%)"
    )
    expect(css).toContain(".markdown-content .chat-code-block-language")
    expect(css).toContain("font-size: 0.75rem")
    expect(css).toContain(".markdown-content .chat-code-block-copy")
    expect(css).toContain("font-size: 0.75rem")
    expect(css).toContain("white-space: pre")
    expect(css).toContain("overflow-x: auto")
    expect(css).toContain(".markdown-content .table-node-wrapper")
    expect(css).toContain("-webkit-overflow-scrolling: touch")
    expect(css).toContain(".markdown-content .table-node th")
    expect(css).toContain(".markdown-content .table-node td")
    expect(css).toContain("font-size: 0.8125rem")
  })
})
