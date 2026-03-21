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

  test("keeps markdown typography contract for dense chat reading", () => {
    const css = loadMarkdownCss()

    expect(css).toContain(".markdown-content .heading-1")
    expect(css).toContain(".markdown-content .heading-2")
    expect(css).toContain(".markdown-content .heading-3")
    expect(css).toContain("font-size: 1.125rem")
    expect(css).toContain("font-size: 1rem")
    expect(css).toContain("font-size: 0.9375rem")
    expect(css).toContain(".markdown-content .list-node")
    expect(css).toContain(".markdown-content .list-item")
    expect(css).toContain(".markdown-content .chat-code-block-language")
    expect(css).toContain("font-size: 0.75rem")
    expect(css).toContain(".markdown-content .chat-code-block-copy")
    expect(css).toContain("font-size: 0.75rem")
    expect(css).toContain(".markdown-content .table-node th")
    expect(css).toContain(".markdown-content .table-node td")
    expect(css).toContain("font-size: 0.8125rem")
  })
})
