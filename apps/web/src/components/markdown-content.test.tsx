import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import { MarkdownRenderer } from "@/components/markdown-content-rich"

describe("MarkdownRenderer", () => {
  test("renders basic markdown structure", () => {
    const html = renderToStaticMarkup(
      <MarkdownRenderer
        content={"# Title\n\nParagraph with `code` and **bold**."}
      />
    )

    expect(html).toContain("Title")
    expect(html).toContain("Paragraph with")
    expect(html).toContain("code")
    expect(html).toContain("<strong")
  })
})
