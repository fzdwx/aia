import { MultiFileDiff, PatchDiff, Virtualizer } from "@pierre/diffs/react"
import type { FileContents } from "@pierre/diffs"
import { useMemo, useState, useRef, useEffect, useTransition, type ReactNode } from "react"

import { useTheme } from "@/components/theme-provider"

import {
  PIERRE_DIFF_HOST_STYLE,
  PIERRE_DIFF_UNSAFE_CSS,
  PIERRE_VIRTUALIZER_CONFIG,
} from "./pierre-config"

function usePierreDiffOptions(diffStyle: "unified" | "split") {
  const { resolvedTheme } = useTheme()

  return useMemo(
    () => ({
      theme: { dark: "pierre-dark", light: "pierre-light" },
      themeType: resolvedTheme,
      diffStyle,
      diffIndicators: "bars" as const,
      lineHoverHighlight: "both" as const,
      disableBackground: false,
      expansionLineCount: 20,
      hunkSeparators: "line-info-basic" as const,
      lineDiffType: "none" as const,
      maxLineDiffLength: 1000,
      maxLineLengthForHighlighting: 1000,
      unsafeCSS: PIERRE_DIFF_UNSAFE_CSS,
      overflow: "wrap" as const,
      disableFileHeader: true,
    }),
    [diffStyle, resolvedTheme]
  )
}

function createContentCacheKey(
  fileName: string,
  side: "old" | "new",
  content: string
) {
  let hash = 2166136261

  for (let index = 0; index < content.length; index += 1) {
    hash ^= content.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }

  return `${fileName}:${side}:${content.length}:${(hash >>> 0).toString(16)}`
}

function PierreDiffScrollContainer({ children }: { children: ReactNode }) {
  return (
    <Virtualizer
      config={PIERRE_VIRTUALIZER_CONFIG}
      className="tool-timeline-pierre-virtualizer"
      contentClassName="tool-timeline-pierre-virtualizer-content"
    >
      {children}
    </Virtualizer>
  )
}

// 通用的延迟渲染 hook
function useDeferredRender() {
  const [shouldRender, setShouldRender] = useState(false)
  const [isPending, startTransition] = useTransition()
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const element = ref.current
    if (!element) return

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          startTransition(() => {
            setShouldRender(true)
          })
          observer.disconnect()
        }
      },
      { rootMargin: "200px" }
    )

    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  return { ref, shouldRender, isPending }
}

// 通用的延迟渲染容器
function DeferredDiffContainer({
  children,
}: {
  children: ReactNode
}) {
  const { ref, shouldRender, isPending } = useDeferredRender()

  return (
    <div ref={ref} className="tool-timeline-patch-diff-container">
      {shouldRender ? (
        children
      ) : (
        <div className="tool-timeline-patch-diff-placeholder" aria-hidden="true">
          {isPending ? "Loading..." : ""}
        </div>
      )}
    </div>
  )
}

export function PierrePatchDiffOutput({ patch }: { patch: string }) {
  const options = usePierreDiffOptions("unified")

  return (
    <PierreDiffScrollContainer>
      <PatchDiff
        patch={patch}
        options={options}
        className="tool-timeline-pierre-root tool-timeline-pierre-root-patch"
        style={PIERRE_DIFF_HOST_STYLE}
      />
    </PierreDiffScrollContainer>
  )
}

// 延迟版本的 PatchDiff
export function DeferredPierrePatchDiffOutput({ patch }: { patch: string }) {
  return (
    <DeferredDiffContainer>
      <PierrePatchDiffOutput patch={patch} />
    </DeferredDiffContainer>
  )
}

export function PierreMultiFileDiffOutput({
  fileName,
  oldContent,
  newContent,
  diffStyle,
}: {
  fileName: string
  oldContent: string
  newContent: string
  diffStyle: "unified" | "split"
}) {
  const options = usePierreDiffOptions(diffStyle)
  const oldFile = useMemo<FileContents>(
    () => ({
      name: fileName,
      contents: oldContent,
      cacheKey: createContentCacheKey(fileName, "old", oldContent),
    }),
    [fileName, oldContent]
  )
  const newFile = useMemo<FileContents>(
    () => ({
      name: fileName,
      contents: newContent,
      cacheKey: createContentCacheKey(fileName, "new", newContent),
    }),
    [fileName, newContent]
  )

  return (
    <PierreDiffScrollContainer>
      <MultiFileDiff
        oldFile={oldFile}
        newFile={newFile}
        options={options}
        className="tool-timeline-pierre-root tool-timeline-pierre-root-multi"
        style={PIERRE_DIFF_HOST_STYLE}
      />
    </PierreDiffScrollContainer>
  )
}

// 延迟版本的 MultiFileDiff
export function DeferredPierreMultiFileDiffOutput({
  fileName,
  oldContent,
  newContent,
  diffStyle,
}: {
  fileName: string
  oldContent: string
  newContent: string
  diffStyle: "unified" | "split"
}) {
  return (
    <DeferredDiffContainer>
      <PierreMultiFileDiffOutput
        fileName={fileName}
        oldContent={oldContent}
        newContent={newContent}
        diffStyle={diffStyle}
      />
    </DeferredDiffContainer>
  )
}
