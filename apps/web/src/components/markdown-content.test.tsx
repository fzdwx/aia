import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, test } from "vite-plus/test"

import {
  computeStreamingMarkdownBlocks,
  MarkdownRenderer,
} from "@/components/markdown-content-rich"

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
    expect(html).toContain('class="inline-code"')
  })

  test("renders code blocks through streamdown container markup", () => {
    const html = renderToStaticMarkup(
      <MarkdownRenderer content={"```ts\nconst answer = 42\n```"} />
    )

    expect(html).toContain("const answer = 42")
    expect(html).toContain('data-streamdown="code-block"')
  })

  test("renders tables with streamdown table wrapper", () => {
    const html = renderToStaticMarkup(
      <MarkdownRenderer
        content={"| Name | Value |\n| --- | --- |\n| alpha | beta |"}
      />
    )

    expect(html).toContain('data-streamdown="table-wrapper"')
    expect(html).toContain("alpha")
    expect(html).toContain("beta")
  })

  test("renders mermaid blocks with diagram controls", () => {
    const html = renderToStaticMarkup(
      <MarkdownRenderer
        content={"```mermaid\nflowchart TD\nA[Start] --> B[End]\n```"}
      />
    )

    expect(html).toContain("animate-spin")
    expect(html).not.toContain("flowchart TD")
  })

  test("renders inline math with katex markup", () => {
    const html = renderToStaticMarkup(
      <MarkdownRenderer content={"Euler:\n\n$$e^{i\\pi}+1=0$$"} />
    )

    expect(html).toContain("katex")
    expect(html).toContain("Euler")
  })

  test("falls back to plain text for oversized streaming markdown", () => {
    const content = `${"a".repeat(16_000)}**tail**`
    const html = renderToStaticMarkup(
      <MarkdownRenderer content={content} streaming />
    )

    expect(html).toContain("tail")
    expect(html).not.toContain("data-streamdown")
    expect(html).not.toContain("<strong")
  })

  test("streaming block cache only reparses the tail block on append", () => {
    const initial = computeStreamingMarkdownBlocks(
      "# Title\n\nFirst paragraph.\n\nSecond",
      null
    )

    expect(initial.blocks).toEqual([
      "# Title\n\n",
      "First paragraph.",
      "\n\n",
      "Second",
    ])

    const appended = computeStreamingMarkdownBlocks(
      "# Title\n\nFirst paragraph.\n\nSecond line extended",
      initial
    )

    expect(appended.blocks).toEqual([
      "# Title\n\n",
      "First paragraph.",
      "\n\n",
      "Second line extended",
    ])
  })

  test("streaming block cache falls back to full parse when prefix changes", () => {
    const initial = computeStreamingMarkdownBlocks(
      "Paragraph one\n\nParagraph two",
      null
    )

    const replaced = computeStreamingMarkdownBlocks(
      "Changed completely\n\nParagraph two",
      initial
    )

    expect(replaced.blocks).toEqual([
      "Changed completely",
      "\n\n",
      "Paragraph two",
    ])
  })
})
