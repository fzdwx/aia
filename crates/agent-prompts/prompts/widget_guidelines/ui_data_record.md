### 3. Data record — bounded UI object

*"Show me a Salesforce contact card" / "Create a receipt for this order"*

Use `widgetRenderer`. Wrap the entire thing in a single raised card (use the `.card` class or apply card styles). All
content is sans-serif since it's pure UI. Use an avatar/initials circle for people (see example below).

```html
<div style="background: var(--card); border-radius: calc(var(--radius) * 1.5); border: 0.5px solid var(--border); padding: 1rem 1.25rem;">
  <div style="display: flex; align-items: center; gap: 12px; margin-bottom: 16px;">
    <div style="width: 44px; height: 44px; border-radius: 50%; background: var(--muted); display: flex; align-items: center; justify-content: center; font-weight: 500; font-size: 14px; color: var(--foreground);">MR</div>
    <div>
      <p style="font-weight: 500; margin: 0;">Maya Rodriguez</p>
      <p style="color: var(--muted-foreground); margin: 0;">VP of Engineering</p>
    </div>
  </div>
  <div style="border-top: 0.5px solid var(--border); padding-top: 12px;">
    <table style="width: 100%;">
      <tr><td style="color: var(--muted-foreground); padding: 4px 0;">Email</td><td style="text-align: right; padding: 4px 0; color: hsl(var(--primary));">m.rodriguez@acme.com</td></tr>
      <tr><td style="color: var(--muted-foreground); padding: 4px 0;">Phone</td><td style="text-align: right; padding: 4px 0;">+1 (415) 555-0172</td></tr>
    </table>
  </div>
</div>
```

