import type { CSSProperties } from "react"

export const PIERRE_DIFF_UNSAFE_CSS = `
:host,
:host [data-diff],
:host [data-file],
:host [data-diffs-header],
:host [data-error-wrapper],
:host [data-virtualizer-buffer] {
  --diffs-bg: transparent;
  --diffs-bg-buffer-override: transparent;
  --diffs-bg-hover-override: transparent;
  --diffs-bg-context-override: transparent;
  --diffs-bg-context-number-override: transparent;
  --diffs-bg-separator-override: transparent;
}

:host {
  overflow: hidden;
  scrollbar-width: none;
  -ms-overflow-style: none;
}

:host pre,
:host code {
  background-color: transparent;
}

:host ::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
  appearance: none;
}

:host ::-webkit-scrollbar-track,
:host ::-webkit-scrollbar-thumb,
:host ::-webkit-scrollbar-corner {
  background: transparent;
  border: 0;
}

* {
  scrollbar-width: none;
  -ms-overflow-style: none;
}

*::-webkit-scrollbar {
  width: 0;
  height: 0;
  display: none;
}
`

export const PIERRE_DIFF_HOST_STYLE: CSSProperties &
  Record<`--${string}`, string> = {
  background: "transparent",
  overflow: "hidden",
  "--aia-diff-surface": "transparent",
  "--diffs-bg": "transparent",
  "--diffs-bg-buffer-override": "transparent",
  "--diffs-bg-hover-override": "transparent",
  "--diffs-fg-number-override": "var(--text-weak)",
  "--diffs-fg-number-addition-override": "var(--text-weak)",
  "--diffs-fg-number-deletion-override": "var(--text-weak)",
  "--diffs-fg-conflict-marker-override": "var(--text-weak)",
  "--shiki-background": "transparent",
  "--diffs-font-family": "var(--font-mono)",
  "--diffs-font-size": "var(--font-size-meta)",
  "--diffs-line-height": "24px",
  "--diffs-tab-size": "2",
  "--diffs-header-font-family": "var(--font-sans)",
  "--diffs-gap-block": "0",
  "--diffs-min-number-column-width": "4ch",
}

export const PIERRE_VIRTUALIZER_CONFIG = {
  overscrollSize: 1200,
  intersectionObserverMargin: 600,
} as const
