import type {
  ChannelListItem,
  ChannelTransport,
  SupportedChannelDefinition,
} from "@/lib/types"

export type ChannelSchemaProperty = Record<string, unknown>

export type ChannelFormState = {
  enabled: boolean
  config: Record<string, unknown>
}

export type ChannelTargetSummary = {
  transportLabel: string
  transportKey: string
  profileLabel: string
  profileState: "draft" | "saved"
  profileCount: number
  multipleProfiles: boolean
}

export function cloneValue(value: unknown): unknown {
  if (typeof structuredClone === "function") return structuredClone(value)
  return JSON.parse(JSON.stringify(value)) as unknown
}

export function schemaProperties(
  definition: SupportedChannelDefinition | null
): Array<[string, ChannelSchemaProperty]> {
  const properties = definition?.config_schema.properties
  if (!properties || typeof properties !== "object") return []
  return Object.entries(properties).map(([key, value]) => [
    key,
    typeof value === "object" && value ? (value as ChannelSchemaProperty) : {},
  ])
}

export function requiredFieldKeys(
  definition: SupportedChannelDefinition | null
): Set<string> {
  const required = definition?.config_schema.required
  if (!Array.isArray(required)) return new Set()
  return new Set(
    required.filter((value): value is string => typeof value === "string")
  )
}

export function fieldKind(
  schema: ChannelSchemaProperty
): "text" | "secret" | "boolean" | "url" {
  if (schema["x-secret"] === true) return "secret"
  if (schema.type === "boolean") return "boolean"
  if (schema.format === "uri") return "url"
  return "text"
}

export function fieldLabel(key: string, schema: ChannelSchemaProperty): string {
  const label = schema["x-label"]
  if (typeof label === "string" && label.trim().length > 0) return label
  return key.replaceAll("_", " ")
}

export function definitionDefaults(
  definition: SupportedChannelDefinition | null
): Record<string, unknown> {
  return Object.fromEntries(
    schemaProperties(definition).map(([key, schema]) => [
      key,
      cloneValue(
        schema.default ?? (fieldKind(schema) === "boolean" ? false : "")
      ),
    ])
  )
}

export function normalizeConfigForForm(
  definition: SupportedChannelDefinition | null,
  config: Record<string, unknown>
): Record<string, unknown> {
  const merged = definitionDefaults(definition)
  for (const [key, schema] of schemaProperties(definition)) {
    if (fieldKind(schema) === "boolean") {
      merged[key] = config[key] === true
      continue
    }
    merged[key] = typeof config[key] === "string" ? config[key] : ""
  }
  return merged
}

export function emptyChannelForm(
  definition: SupportedChannelDefinition | null
): ChannelFormState {
  return {
    enabled: true,
    config: definitionDefaults(definition),
  }
}

export function buildChannelFormState(
  definition: SupportedChannelDefinition | null,
  configuredChannel: ChannelListItem | null
): ChannelFormState {
  if (definition && configuredChannel) {
    return {
      enabled: configuredChannel.enabled,
      config: normalizeConfigForForm(definition, configuredChannel.config),
    }
  }

  return emptyChannelForm(definition)
}

export function isMissingRequiredValue(
  key: string,
  schema: ChannelSchemaProperty,
  value: unknown,
  editing: boolean,
  requiredKeys: Set<string>
): boolean {
  if (!requiredKeys.has(key)) return false
  if (editing && fieldKind(schema) === "secret") return false
  if (fieldKind(schema) === "boolean") return false
  return typeof value !== "string" || value.trim().length === 0
}

export function configuredChannelsForTransport(
  configuredChannels: ChannelListItem[],
  transport: ChannelTransport | null
): ChannelListItem[] {
  if (!transport) return []
  return configuredChannels.filter((channel) => channel.transport === transport)
}

export function configFieldCount(
  definition: SupportedChannelDefinition | null
): number {
  const properties = definition?.config_schema.properties
  if (!properties || typeof properties !== "object") return 0
  return Object.keys(properties).length
}

export function collectFieldIssues(
  definition: SupportedChannelDefinition | null,
  config: Record<string, unknown>,
  editing: boolean
): Record<string, string> {
  const issues: Record<string, string> = {}
  const requiredKeys = requiredFieldKeys(definition)

  for (const [key, schema] of schemaProperties(definition)) {
    if (
      isMissingRequiredValue(key, schema, config[key], editing, requiredKeys)
    ) {
      issues[key] = "This field is required before you can save this profile."
    }
  }

  return issues
}

export function summarizeChannelTarget(
  definition: SupportedChannelDefinition | null,
  configuredChannel: ChannelListItem | null,
  matchingChannels: ChannelListItem[]
): ChannelTargetSummary {
  return {
    transportLabel: definition?.label ?? "Unknown transport",
    transportKey: definition?.transport ?? "unconfigured",
    profileLabel: configuredChannel?.id ?? "Draft profile",
    profileState: configuredChannel ? "saved" : "draft",
    profileCount: matchingChannels.length,
    multipleProfiles: matchingChannels.length > 1,
  }
}

export function buildDeleteConfirmationCopy(
  summary: ChannelTargetSummary
): string {
  return `Delete profile ${summary.profileLabel} for transport ${summary.transportKey}?`
}
