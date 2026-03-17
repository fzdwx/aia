import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../ui"

export function createShellRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "shell",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return truncateInline(
        getStringValue(args, "command", "cmd") ??
          getToolDisplayPath(data.toolName, data.details, args),
        120
      )
    },
    renderDetails(data) {
      const stdout = getStringValue(data.details, "stdout")
      const stderr = getStringValue(data.details, "stderr")
      const exitCode = getNumberValue(data.details, "exit_code")
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Execution">
            <DetailList entries={[{ label: "Exit code", value: exitCode }]} />
          </ToolDetailSection>
          {stdout ? (
            <ToolDetailSection title="Stdout">
              <ExpandableOutput value={stdout} failed={false} />
            </ToolDetailSection>
          ) : null}
          {stderr ? (
            <ToolDetailSection title="Stderr">
              <ExpandableOutput value={stderr} failed />
            </ToolDetailSection>
          ) : null}
          {!stdout && !stderr && data.outputContent ? (
            <ToolDetailSection title="Outcome">
              <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
