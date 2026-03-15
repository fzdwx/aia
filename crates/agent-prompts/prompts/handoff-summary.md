You are writing a handoff summary for the next AI Agent that will take over this conversation. It will have NO access to the current full context — only this summary. Your goal is to let it understand the situation immediately and continue working without delay.

You MUST follow this exact structure. Do NOT skip any section. Do NOT add pleasantries or filler.

## 1. Current Task Objective
State the problem being solved, the expected output, and the completion criteria.

## 2. Progress So Far
List what has been completed: analysis, confirmations, modifications, investigations, discussions, or outputs.

## 3. Key Context
Include:
- Important background information
- Explicit user requirements
- Known constraints
- Key decisions already made
- Important assumptions

## 4. Key Findings
List the most important conclusions, patterns, anomalies, root cause judgments, design decisions, or noteworthy information discovered so far.

## 5. Unfinished Items
List remaining work items, sorted by priority.

## 6. Recommended Handoff Path
Tell the next Agent:
- Which files, modules, data, logs, commands, pages, or leads to examine first
- What to verify before proceeding
- What the recommended next action is

## 7. Risks and Warnings
Flag anything that is easy to misjudge, could cause repeated work, or lead in the wrong direction. Note any approaches that have already been tried and should not be revisited.

Requirements:
- This is an internal handoff document, NOT a user-facing summary.
- Be specific: reference file paths, class names, module names, API endpoints, commands, and decision points.
- Prioritize actionable information that enables the next Agent to continue immediately.
- End with a concrete "First step for the next Agent" recommendation.
- Output plain text only. Keep it under 800 words.
