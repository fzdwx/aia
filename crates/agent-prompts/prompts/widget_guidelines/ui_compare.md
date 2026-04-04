### 2. Compare options — decision making

*"Compare pricing and features of these products" / "Help me choose between React and Vue"*

Use `widgetRenderer`. Side-by-side card grid for options. Highlight differences with semantic colors. Interactive
elements for filtering or weighting.

- Use `repeat(auto-fit, minmax(160px, 1fr))` for responsive columns
- Each option in a card (use the `.card` class). Use badges (`.badge` class) for key differentiators.
- Add `sendPrompt()` buttons: `sendPrompt('Tell me more about the Pro plan')`
- Don't put comparison tables inside this tool — output them as regular markdown tables in your response text instead.
  The tool is for the visual card grid only.
- When one option is recommended or "most popular", accent its card with `border: 2px solid hsl(var(--primary))` only (
  2px is deliberate — the only exception to the 0.5px rule, used to accent featured items) — keep the same background
  and border as the other cards. Add a small `.badge.primary` (e.g. "Most popular") above or inside the card header.

