import { parseMarkdownIntoBlocks } from "streamdown"

export type StreamingMarkdownBlockCache = {
  content: string
  blocks: string[]
}

export function computeStreamingMarkdownBlocks(
  content: string,
  previous: StreamingMarkdownBlockCache | null
): StreamingMarkdownBlockCache {
  if (!previous || previous.blocks.length === 0) {
    return {
      content,
      blocks: parseMarkdownIntoBlocks(content),
    }
  }

  if (!content.startsWith(previous.content)) {
    return {
      content,
      blocks: parseMarkdownIntoBlocks(content),
    }
  }

  const suffix = content.slice(previous.content.length)
  if (suffix.length === 0) {
    return previous
  }

  const stableBlocks = previous.blocks.slice(0, -1)
  const previousTail = previous.blocks[previous.blocks.length - 1] ?? ""
  const reparsedTail = parseMarkdownIntoBlocks(previousTail + suffix)

  return {
    content,
    blocks: [...stableBlocks, ...reparsedTail],
  }
}
