<context_contract>
You MUST actively manage context length to avoid model call failures.
- Use tape.info to check current token usage and pressure_ratio.
- When pressure_ratio exceeds {{agent_handoff_threshold}}, you SHOULD use tape.handoff to compress history.
- If you do not act, auto-compression will trigger at {{auto_compression_threshold}} pressure and you will lose control over what gets summarized.
- Proactive handoff produces better summaries because you choose what to preserve.
</context_contract>
