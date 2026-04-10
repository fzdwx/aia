import { useState } from "react"
import { Eye, EyeOff, Plus, X } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

export type ModelFormRow = {
  _key: string
  id: string
  display_name: string
  limit_context: string
  limit_output: string
  supports_reasoning: boolean
}

export function ProviderConnectionSection({
  name,
  kind,
  providerNameInputId,
  providerProtocolInputId,
  providerProtocolLabelId,
  selectedProviderLocked,
  onNameChange,
  onKindChange,
}: {
  name: string
  kind: string
  providerNameInputId: string
  providerProtocolInputId: string
  providerProtocolLabelId: string
  selectedProviderLocked: boolean
  onNameChange: (value: string) => void
  onKindChange: (value: string | null) => void
}) {
  return (
    <section className="rounded-xl border border-border/30 bg-card/70 p-3">
      <p className="workspace-section-label text-foreground">Connection</p>

      <div className="mt-2.5 grid gap-2 sm:grid-cols-2">
        <div className="space-y-1.5">
          <label htmlFor={providerNameInputId} className="workspace-form-label">
            Name
          </label>
          <Input
            id={providerNameInputId}
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            placeholder="e.g. openai-main"
            className="h-8"
            disabled={selectedProviderLocked}
          />
        </div>

        <div className="space-y-1.5">
          <label
            id={providerProtocolLabelId}
            htmlFor={providerProtocolInputId}
            className="workspace-form-label"
          >
            Protocol
          </label>
          <Select value={kind} onValueChange={(value) => onKindChange(value)}>
            <SelectTrigger
              id={providerProtocolInputId}
              aria-labelledby={providerProtocolLabelId}
              className="h-8 w-full"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="openai-responses">Responses API</SelectItem>
              <SelectItem value="openai-chat-completions">
                Chat Completions API
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>
    </section>
  )
}

export function ProviderAuthenticationSection({
  selectedProvider,
  providerBaseUrlInputId,
  providerApiKeyInputId,
  apiKey,
  baseUrl,
  onBaseUrlChange,
  onApiKeyChange,
}: {
  selectedProvider: boolean
  providerBaseUrlInputId: string
  providerApiKeyInputId: string
  apiKey: string
  baseUrl: string
  onBaseUrlChange: (value: string) => void
  onApiKeyChange: (value: string) => void
}) {
  const [showKey, setShowKey] = useState(false)

  return (
    <section className="rounded-xl border border-border/30 bg-card/70 p-3">
      <p className="workspace-section-label text-foreground">
        Authentication
      </p>

      <div className="mt-2.5 grid gap-2 sm:grid-cols-2">
        <div className="space-y-1.5">
          <label
            htmlFor={providerBaseUrlInputId}
            className="workspace-form-label"
          >
            Base URL
          </label>
          <Input
            id={providerBaseUrlInputId}
            value={baseUrl}
            onChange={(event) => onBaseUrlChange(event.target.value)}
            className="h-8"
          />
        </div>

        <div className="space-y-1.5">
          <label
            htmlFor={providerApiKeyInputId}
            className="workspace-form-label"
          >
            API key
          </label>
          <div className="relative">
            <Input
              id={providerApiKeyInputId}
              type={showKey ? "text" : "password"}
              value={apiKey}
              onChange={(event) => onApiKeyChange(event.target.value)}
              placeholder={selectedProvider ? "Leave blank to keep current key" : "sk-..."}
              className="h-8 pr-9"
            />
            <button
              type="button"
              onClick={() => setShowKey((prev) => !prev)}
              className="absolute top-0 right-0 flex size-8 items-center justify-center text-muted-foreground/60 transition-colors hover:text-foreground"
              aria-label={showKey ? "Hide API key" : "Show API key"}
              tabIndex={-1}
            >
              {showKey ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
            </button>
          </div>
        </div>
      </div>
    </section>
  )
}

export function ProviderModelCatalogSection({
  modelRowsWithId,
  models,
  settingsScopeId,
  onAddModel,
  onUpdateModelRow,
  onRemoveModelRow,
}: {
  modelRowsWithId: number
  models: ModelFormRow[]
  settingsScopeId: string
  onAddModel: () => void
  onUpdateModelRow: (index: number, patch: Partial<ModelFormRow>) => void
  onRemoveModelRow: (index: number) => void
}) {
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set())

  function toggleExpanded(key: string) {
    setExpandedKeys((prev) => {
      const next = new Set(prev)
      if (next.has(key)) {
        next.delete(key)
      } else {
        next.add(key)
      }
      return next
    })
  }

  return (
    <section className="rounded-xl border border-border/30 bg-card/70 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <p className="workspace-section-label text-foreground">
            Models
          </p>
          <span className="workspace-code rounded-sm border border-border/30 px-1.5 py-0.5 text-muted-foreground">
            {modelRowsWithId}
          </span>
        </div>

        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onAddModel}
          className="h-8 px-2.5"
        >
          <Plus className="size-3.5" />
          Add model
        </Button>
      </div>

      <div className="mt-2.5 space-y-1.5">
        {models.map((row, index) => {
          const isExpanded = expandedKeys.has(row._key) || !row.id.trim()

          return (
            <div
              key={row._key}
              className="rounded-lg border border-border/25 bg-background/60"
            >
              <div className="flex items-center gap-2 px-2.5 py-2">
                {row.id.trim() ? (
                  <button
                    type="button"
                    onClick={() => toggleExpanded(row._key)}
                    className="flex min-w-0 flex-1 items-center gap-2 text-left"
                  >
                    <span className="text-ui-sm truncate font-mono font-medium text-foreground">
                      {row.id}
                    </span>
                    {row.display_name.trim() ? (
                      <span className="text-ui-xs truncate text-muted-foreground">
                        {row.display_name}
                      </span>
                    ) : null}
                    {row.supports_reasoning ? (
                      <span className="text-ui-xs rounded-sm border border-border/30 px-1 py-0.5 text-muted-foreground">
                        reasoning
                      </span>
                    ) : null}
                    <span className="text-ui-xs ml-auto text-muted-foreground/50">
                      {isExpanded ? "Collapse" : "Expand"}
                    </span>
                  </button>
                ) : (
                  <div className="flex min-w-0 flex-1 items-center">
                    <Input
                      id={`${settingsScopeId}-model-id-${index}`}
                      value={row.id}
                      onChange={(event) =>
                        onUpdateModelRow(index, { id: event.target.value })
                      }
                      placeholder="Model ID"
                      className="h-7 text-ui-sm"
                      aria-label={`Model ${index + 1} ID`}
                    />
                  </div>
                )}

                {models.length > 1 ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => onRemoveModelRow(index)}
                    aria-label={`Remove model ${index + 1}`}
                    className="size-7 shrink-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                  >
                    <X className="size-3.5" />
                  </Button>
                ) : null}
              </div>

              {isExpanded ? (
                <div className="grid gap-2 border-t border-border/15 px-2.5 py-2 sm:grid-cols-4">
                  {row.id.trim() ? null : (
                    <div className="space-y-1.5 sm:col-span-4">
                      <label className="workspace-form-label">Model ID</label>
                    </div>
                  )}

                  <div className="space-y-1.5">
                    <label className="workspace-form-label">Display name</label>
                    <Input
                      id={`${settingsScopeId}-model-display-name-${index}`}
                      value={row.display_name}
                      onChange={(event) =>
                        onUpdateModelRow(index, {
                          display_name: event.target.value,
                        })
                      }
                      placeholder="Optional"
                      className="h-7 text-ui-sm"
                      aria-label={`Model ${index + 1} display name`}
                    />
                  </div>

                  <div className="space-y-1.5">
                    <label className="workspace-form-label">Context</label>
                    <Input
                      id={`${settingsScopeId}-model-context-limit-${index}`}
                      value={row.limit_context}
                      onChange={(event) =>
                        onUpdateModelRow(index, {
                          limit_context: event.target.value,
                        })
                      }
                      placeholder="ctx"
                      className="h-7 text-ui-sm"
                      inputMode="numeric"
                      aria-label={`Model ${index + 1} context limit`}
                    />
                  </div>

                  <div className="space-y-1.5">
                    <label className="workspace-form-label">Output</label>
                    <Input
                      id={`${settingsScopeId}-model-output-limit-${index}`}
                      value={row.limit_output}
                      onChange={(event) =>
                        onUpdateModelRow(index, {
                          limit_output: event.target.value,
                        })
                      }
                      placeholder="out"
                      className="h-7 text-ui-sm"
                      inputMode="numeric"
                      aria-label={`Model ${index + 1} output limit`}
                    />
                  </div>

                  <div className="flex items-center gap-2 pt-4">
                    <Switch
                      checked={row.supports_reasoning}
                      onCheckedChange={(checked: boolean) =>
                        onUpdateModelRow(index, { supports_reasoning: checked })
                      }
                      size="default"
                      aria-label={`Model ${index + 1} reasoning support`}
                    />
                    <label className="workspace-form-label cursor-default">Reasoning</label>
                  </div>
                </div>
              ) : null}
            </div>
          )
        })}
      </div>
    </section>
  )
}
