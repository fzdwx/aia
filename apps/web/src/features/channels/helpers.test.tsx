import { describe, expect, test } from "vite-plus/test"

import type { ChannelListItem, SupportedChannelDefinition } from "@/lib/types"

import {
  buildDeleteConfirmationCopy,
  collectFieldIssues,
  summarizeChannelTarget,
} from "./helpers"

const FEISHU_DEFINITION: SupportedChannelDefinition = {
  transport: "feishu",
  label: "Feishu",
  description: "Sync messages into runtime",
  config_schema: {
    properties: {
      webhook_url: {
        type: "string",
        format: "uri",
        description: "Webhook endpoint",
      },
      signing_secret: {
        type: "string",
        "x-secret": true,
      },
      enabled: {
        type: "boolean",
      },
    },
    required: ["webhook_url", "signing_secret"],
  },
}

const CONFIGURED_PROFILE: ChannelListItem = {
  id: "feishu-primary",
  name: "Feishu",
  transport: "feishu",
  enabled: true,
  config: {
    webhook_url: "https://hooks.example.test/feishu",
    signing_secret: "stored",
    enabled: true,
  },
  secret_fields_set: ["signing_secret"],
}

describe("channel panel helpers", () => {
  test("summarizes draft transport state before the first profile exists", () => {
    const summary = summarizeChannelTarget(FEISHU_DEFINITION, null, [])

    expect(summary.transportLabel).toBe("Feishu")
    expect(summary.transportKey).toBe("feishu")
    expect(summary.profileLabel).toBe("Draft profile")
    expect(summary.profileState).toBe("draft")
    expect(summary.profileCount).toBe(0)
    expect(summary.multipleProfiles).toBe(false)
  })

  test("summarizes saved profile state and multiple-profile warning", () => {
    const summary = summarizeChannelTarget(
      FEISHU_DEFINITION,
      CONFIGURED_PROFILE,
      [CONFIGURED_PROFILE, { ...CONFIGURED_PROFILE, id: "feishu-secondary" }]
    )

    expect(summary.profileLabel).toBe("feishu-primary")
    expect(summary.profileState).toBe("saved")
    expect(summary.profileCount).toBe(2)
    expect(summary.multipleProfiles).toBe(true)
  })

  test("flags required draft fields but keeps edit-time secrets valid when blank", () => {
    const draftIssues = collectFieldIssues(
      FEISHU_DEFINITION,
      {
        webhook_url: "",
        signing_secret: "",
        enabled: false,
      },
      false
    )

    expect(draftIssues.webhook_url).toContain("required")
    expect(draftIssues.signing_secret).toContain("required")

    const editIssues = collectFieldIssues(
      FEISHU_DEFINITION,
      {
        webhook_url: "https://hooks.example.test/feishu",
        signing_secret: "",
        enabled: true,
      },
      true
    )

    expect(editIssues.webhook_url).toBeUndefined()
    expect(editIssues.signing_secret).toBeUndefined()
  })

  test("builds delete confirmation copy for the exact saved profile", () => {
    const summary = summarizeChannelTarget(
      FEISHU_DEFINITION,
      CONFIGURED_PROFILE,
      [CONFIGURED_PROFILE]
    )

    expect(buildDeleteConfirmationCopy(summary)).toContain("feishu-primary")
    expect(buildDeleteConfirmationCopy(summary)).toContain("feishu")
  })
})
