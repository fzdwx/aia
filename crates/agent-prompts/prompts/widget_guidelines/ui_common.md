## UI components

### Aesthetic

Flat, clean surfaces. Minimal 0.5px borders. Generous whitespace. No gradients, no shadows (except functional focus
rings). Everything should feel native to the host app — like it belongs on the page, not embedded from somewhere else.

### Tokens

- Borders: always `0.5px solid var(--border)` (or with higher opacity for emphasis)
- Corner radius: `var(--radius)` for most elements, `calc(var(--radius) * 1.5)` for cards
- Cards: `var(--card)` bg, 0.5px border, radius * 1.5, padding 1rem 1.25rem
- Form elements (input, select, textarea, button, range slider) are pre-styled — write bare tags. Text inputs are 36px
  with hover/focus built in; range sliders have 4px track + 18px thumb; buttons have outline style with hover/active.
  Only add inline styles to override (e.g., different width). **DO NOT set font-size, font-weight, or color on these
  elements** — they inherit the host's design system.
- Buttons: pre-styled with transparent bg, 0.5px border, hover muted bg, active scale(0.98). Use `button.primary` for
  filled primary buttons. Use `button.destructive` for outline destructive buttons. If it triggers sendPrompt, append a
  ↗ arrow.
- **Round every displayed number.** JS float math leaks artifacts — `0.1 + 0.2` gives `0.30000000000000004`, `7 * 1.1`
  gives `7.700000000000001`. Any number that reaches the screen (slider readouts, stat card values, axis labels,
  data-point labels, tooltips, computed totals) must go through `Math.round()`, `.toFixed(n)`, or `Intl.NumberFormat`.
  Pick the precision that makes sense for the context — integers for counts, 1–2 decimals for percentages,
  `toLocaleString()` for currency. For range sliders, also set `step="1"` (or step="0.1" etc.) so the input itself emits
  round values.
- Spacing: use rem for vertical rhythm (1rem, 1.5rem, 2rem), px for component-internal gaps (8px, 12px, 16px)
- Box-shadows: none, except `box-shadow: 0 0 0 Npx` focus rings on inputs

### Metric cards

For summary numbers (revenue, count, percentage) — surface card with muted 13px label above, large number below (use a
`<span>` for the number and set font-size: 24px; font-weight: 500 on that span — this is a custom element, not a
pre-styled tag). `background: var(--muted)`, no border, `border-radius: var(--radius)`, padding 1rem. Use in grids of
2-4 with `gap: 12px`. Distinct from raised cards (which have card bg + border).

### Layout

- Editorial (explanatory content): no card wrapper, prose flows naturally
- Card (bounded objects like a contact record, receipt): single raised card wraps the whole thing (use the `.card` class
  or apply card styles manually)
- Don't put tables here — output them as markdown in your response text

**Grid overflow:** `grid-template-columns: 1fr` has `min-width: auto` by default — children with large min-content push
the column past the container. Use `minmax(0, 1fr)` to clamp.

**Table overflow:** Tables with many columns auto-expand past `width: 100%` if cell contents exceed it. In constrained
layouts (≤700px), use `table-layout: fixed` and set explicit column widths, or reduce columns, or allow horizontal
scroll on a wrapper.

### Mockup presentation

Contained mockups — mobile screens, chat threads, single cards, modals, small UI components — should sit on a background
surface (`var(--muted)` container with `border-radius: calc(var(--radius) * 1.5)` and padding, or a device frame) so
they don't float naked on the widget canvas. Full-width mockups like dashboards, settings pages, or data tables that
naturally fill the viewport do not need an extra wrapper.

