import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { MarkdownRenderer } from "@/components/markdown-content-rich"
import { ThemeProvider } from "@/components/theme-provider"

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
})
