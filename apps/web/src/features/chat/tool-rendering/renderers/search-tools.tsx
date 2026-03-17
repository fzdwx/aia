import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  getBooleanValue,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { DetailList, ExpandableOutput, ToolDetailSection } from "../ui"

export function createGlobRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "glob",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const pattern = getStringValue(args, "pattern")
      const path = getStringValue(args, "path")
      return [pattern, path ? `in ${path}` : ""].filter(Boolean).join(" — ")
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Matches">
            <DetailList
              entries={[
                {
                  label: "Pattern",
                  value: getStringValue(data.details, "pattern"),
                },
                {
                  label: "Matches",
                  value: getNumberValue(data.details, "matches"),
                },
                {
                  label: "Returned",
                  value: getNumberValue(data.details, "returned"),
                },
                {
                  label: "Limit",
                  value: getNumberValue(data.details, "limit"),
                },
                {
                  label: "Aborted",
                  value: getBooleanValue(data.details, "aborted")
                    ? "yes"
                    : undefined,
                },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Results">
              <ExpandableOutput
                value={data.outputContent}
                failed={!data.succeeded}
              />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}

export function createGrepRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "grep",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const pattern = getStringValue(args, "pattern")
      const path = getStringValue(args, "path")
      const glob = getStringValue(args, "glob")
      return [
        pattern ? truncateInline(pattern, 48) : "",
        path ? `in ${path}` : "",
        glob ? `glob ${glob}` : "",
      ]
        .filter(Boolean)
        .join(" — ")
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Search">
            <DetailList
              entries={[
                {
                  label: "Pattern",
                  value: getStringValue(data.details, "pattern"),
                },
                {
                  label: "Matches",
                  value: getNumberValue(data.details, "matches"),
                },
                {
                  label: "Returned",
                  value: getNumberValue(data.details, "returned"),
                },
                {
                  label: "Limit",
                  value: getNumberValue(data.details, "limit"),
                },
                {
                  label: "Aborted",
                  value: getBooleanValue(data.details, "aborted")
                    ? "yes"
                    : undefined,
                },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Results">
              <ExpandableOutput
                value={data.outputContent}
                failed={!data.succeeded}
              />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
