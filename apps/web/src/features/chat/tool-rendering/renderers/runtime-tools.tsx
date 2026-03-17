import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import { getNumberValue, getStringValue, truncateInline } from "../helpers"
import { DetailList, ExpandableOutput, ToolDetailSection } from "../ui"

export function createTapeInfoRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "tape_info",
    renderTitle(data) {
      const pressureRatio = getNumberValue(data.details, "pressure_ratio")
      return pressureRatio != null
        ? `pressure ${(pressureRatio * 100).toFixed(1)}%`
        : "context usage"
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Context">
            <DetailList
              entries={[
                {
                  label: "Entries",
                  value: getNumberValue(data.details, "entries"),
                },
                {
                  label: "Anchors",
                  value: getNumberValue(data.details, "anchors"),
                },
                {
                  label: "Since last anchor",
                  value: getNumberValue(
                    data.details,
                    "entries_since_last_anchor"
                  ),
                },
                {
                  label: "Last input tokens",
                  value: getNumberValue(data.details, "last_input_tokens"),
                },
                {
                  label: "Context limit",
                  value: getNumberValue(data.details, "context_limit"),
                },
                {
                  label: "Output limit",
                  value: getNumberValue(data.details, "output_limit"),
                },
                {
                  label: "Pressure",
                  value:
                    typeof data.details?.pressure_ratio === "number"
                      ? `${(data.details.pressure_ratio * 100).toFixed(1)}%`
                      : undefined,
                },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Payload">
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

export function createTapeHandoffRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "tape_handoff",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const summary = getStringValue(args, "summary")
      return [
        getStringValue(args, "name") ?? "handoff",
        summary ? truncateInline(summary, 72) : "",
      ]
        .filter(Boolean)
        .join(" — ")
    },
    renderDetails(data) {
      return data.outputContent ? (
        <ToolDetailSection title="Outcome">
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      ) : null
    },
  }
}
