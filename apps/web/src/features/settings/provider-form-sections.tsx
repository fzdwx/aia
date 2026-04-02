import { Plus, X } from "lucide-react"

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
      <div className="mb-2.5">
        <p className="workspace-section-label text-foreground">Connection</p>
        <p className="workspace-panel-copy mt-1 text-muted-foreground">
          The provider name is the registry key and must be unique. The protocol
          controls how requests are mapped.
        </p>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
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
  providerApiKeyHintId,
  baseUrl,
  apiKey,
  onBaseUrlChange,
  onApiKeyChange,
}: {
  selectedProvider: boolean
  providerBaseUrlInputId: string
  providerApiKeyInputId: string
  providerApiKeyHintId: string
  baseUrl: string
  apiKey: string
  onBaseUrlChange: (value: string) => void
  onApiKeyChange: (value: string) => void
}) {
  return (
    <section className="rounded-xl border border-border/30 bg-card/70 p-3">
      <div className="mb-2.5">
        <p className="workspace-section-label text-foreground">
          Authentication
        </p>
        <p className="workspace-panel-copy mt-1 text-muted-foreground">
          Required when creating a provider. Leave this blank while editing to
          keep the current key.
        </p>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
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
          <p className="workspace-form-note">
            This URL defines the request host and path prefix, such as an
            OpenAI-compatible gateway.
          </p>
        </div>

        <div className="space-y-1.5">
          <label
            htmlFor={providerApiKeyInputId}
            className="workspace-form-label"
          >
            API key
          </label>
          {selectedProvider ? (
            <p id={providerApiKeyHintId} className="workspace-form-note">
              Leave blank to keep the current key.
            </p>
          ) : null}
          <Input
            id={providerApiKeyInputId}
            type="text"
            value={apiKey}
            onChange={(event) => onApiKeyChange(event.target.value)}
            placeholder="sk-..."
            aria-describedby={
              selectedProvider ? providerApiKeyHintId : undefined
            }
            className="h-8"
          />
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
  return (
    <section className="rounded-xl border border-border/30 bg-card/70 p-3">
      <div className="mb-2.5 flex flex-wrap items-start justify-between gap-2">
        <div>
          <p className="workspace-section-label text-foreground">
            Model Catalog
          </p>
          <p className="workspace-panel-copy mt-1 text-muted-foreground">
            Add at least one valid model ID. Context and output limits fall back
            to backend defaults when left blank.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <span className="workspace-code rounded-sm border border-border/30 px-1.5 py-0.5 text-muted-foreground">
            {modelRowsWithId} active
          </span>
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
      </div>

      <div className="overflow-x-auto rounded-lg border border-border/25">
        <div className="min-w-[840px]">
          <div className="workspace-section-label grid grid-cols-[minmax(220px,2fr)_minmax(170px,1.4fr)_110px_110px_120px_44px] gap-2 border-b border-border/20 bg-muted/[0.12] px-2.5 py-2 text-muted-foreground">
            <span>Model ID</span>
            <span>Display Name</span>
            <span>Context</span>
            <span>Output</span>
            <span>Reasoning</span>
            <span className="text-center">-</span>
          </div>

          <div className="divide-y divide-border/20">
            {models.map((row, index) => (
              <div
                key={row._key}
                className="grid grid-cols-[minmax(220px,2fr)_minmax(170px,1.4fr)_110px_110px_120px_44px] gap-2 px-2.5 py-2"
              >
                <Input
                  id={`${settingsScopeId}-model-id-${index}`}
                  value={row.id}
                  onChange={(event) =>
                    onUpdateModelRow(index, { id: event.target.value })
                  }
                  placeholder="gpt-5.4"
                  className="h-8"
                  aria-label={`Model ${index + 1} ID`}
                />

                <Input
                  id={`${settingsScopeId}-model-display-name-${index}`}
                  value={row.display_name}
                  onChange={(event) =>
                    onUpdateModelRow(index, {
                      display_name: event.target.value,
                    })
                  }
                  placeholder="Optional display name"
                  className="h-8"
                  aria-label={`Model ${index + 1} display name`}
                />

                <Input
                  id={`${settingsScopeId}-model-context-limit-${index}`}
                  value={row.limit_context}
                  onChange={(event) =>
                    onUpdateModelRow(index, {
                      limit_context: event.target.value,
                    })
                  }
                  placeholder="ctx"
                  className="h-8"
                  inputMode="numeric"
                  aria-label={`Model ${index + 1} context limit`}
                />

                <Input
                  id={`${settingsScopeId}-model-output-limit-${index}`}
                  value={row.limit_output}
                  onChange={(event) =>
                    onUpdateModelRow(index, {
                      limit_output: event.target.value,
                    })
                  }
                  placeholder="out"
                  className="h-8"
                  inputMode="numeric"
                  aria-label={`Model ${index + 1} output limit`}
                />

                <div className="flex items-center justify-center rounded-md border border-border/25 bg-background/60 px-2">
                  <Switch
                    checked={row.supports_reasoning}
                    onCheckedChange={(checked: boolean) =>
                      onUpdateModelRow(index, { supports_reasoning: checked })
                    }
                    size="default"
                    aria-label={`Model ${index + 1} reasoning support`}
                  />
                </div>

                <div className="flex items-center justify-center">
                  {models.length > 1 ? (
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      onClick={() => onRemoveModelRow(index)}
                      aria-label={`Remove model ${index + 1}`}
                      className="size-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    >
                      <X className="size-3.5" />
                    </Button>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <p className="workspace-meta mt-2 text-muted-foreground">
        Turn on Reasoning if this model supports session-level thinking
        controls.
      </p>
    </section>
  )
}
