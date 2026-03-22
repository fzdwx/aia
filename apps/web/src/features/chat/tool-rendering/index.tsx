export type {
  ToolRenderData,
  ToolRenderer,
  ToolRendererRegistry,
} from "./types"

import { createToolRendererRegistry } from "./registry"
import { createDefaultToolRenderer } from "./renderers/default"
import {
  createEditRenderer,
  createReadRenderer,
  createWriteRenderer,
} from "./renderers/file-tools"
import {
  createCodeSearchRenderer,
  createGlobRenderer,
  createGrepRenderer,
  createWebSearchRenderer,
} from "./renderers/search-tools"
import { createShellRenderer } from "./renderers/shell"
import { createApplyPatchRenderer } from "./renderers/apply-patch"
import {
  createTapeHandoffRenderer,
  createTapeInfoRenderer,
} from "./renderers/runtime-tools"

export const toolRendererRegistry = createToolRendererRegistry(
  createDefaultToolRenderer(),
  [
    createReadRenderer(),
    createWriteRenderer(),
    createEditRenderer(),
    createCodeSearchRenderer(),
    createWebSearchRenderer(),
    createGlobRenderer(),
    createGrepRenderer(),
    createShellRenderer(),
    createApplyPatchRenderer(),
    createTapeInfoRenderer(),
    createTapeHandoffRenderer(),
  ]
)
