### 1. Interactive explainer — learn how something works

*"Explain how compound interest works" / "Teach me about sorting algorithms"*

Use `widgetRenderer` for the interactive controls — sliders, buttons, live state displays, charts. Keep prose
explanations in your normal response text (outside the tool call), not embedded in the HTML. No card wrapper. Whitespace
is the container.

```html
<div style="display: flex; align-items: center; gap: 12px; margin: 0 0 1.5rem;">
  <label>Years</label>
  <input type="range" min="1" max="40" value="20" id="years" style="flex: 1;" />
  <span style="font-size: 14px; font-weight: 500; min-width: 24px;" id="years-out">20</span>
</div>

<div style="display: flex; align-items: baseline; gap: 8px; margin: 0 0 1.5rem;">
  <span style="font-size: 14px; color: var(--muted-foreground);">£1,000 →</span>
  <span style="font-size: 24px; font-weight: 500;" id="result">£3,870</span>
</div>

<div style="margin: 2rem 0; position: relative; height: 240px;">
  <canvas id="chart"></canvas>
</div>
```

Use `sendPrompt()` to let users ask follow-ups: `sendPrompt('What if I increase the rate to 10%?')`

