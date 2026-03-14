> 这里是用户记录的问题追踪，ai解决后需要标记完成

# 重要： 上下文自动压缩

如果进行的上下文自动压缩，则这个内容会覆盖 system prompt, 而且感觉生成的内容也不是很详细, 来源条目生成进去干什么？


```json
{
  "content": "当前阶段: context_compression\n锚点摘要: Handoff Note\n\nCurrent state:\n- User first greeted the assistant.\n- User then asked in Chinese: “当前项目里面是干什么的” (“What is this project doing right now?”).\n- Assistant explained the project as an AI Agent runtime/workbench called “aia agent,” with:\n  - Web frontend in `apps/web`\n  - Rust agent server in `apps/agent-server`\n  - Shared runtime/core crates in `crates/`\n  - Session persistence, provider registry, built-in tools, OpenAI-style adapters, and trace support\n- Assistant described the project as being in a “Web UI ↔ runtime bridge” stage and listed current capabilities and future directions.\n- User then sent “hello” again; no response to that greeting has been given yet.\n\nKey decisions:\n- No implementation or code changes were requested.\n- No debugging, planning, or file-level modifications were performed.\n- The conversation stayed at project-understanding / architecture-summary level.\n\nFiles modified:\n- None.\n\nOutstanding tasks:\n- Respond to the user’s latest “hello”.\n- If the user wants more detail, possible follow-ups already offered:\n  - Explain each directory\n  - Walk through the full request/runtime/tool call flow\n  - Identify the 5 most important files to read first\n\nNotes for next assistant:\n- User is comfortable with Chinese; prior substantive answer was in Chinese.\n- Best next step is likely a brief greeting in Chinese and ask what aspect of the project they want to inspect next.\n下一步: \n来源条目: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59]\n所有者: runtime",
  "role": "system"
}
```

需要优化上下文压缩的逻辑


# ~~一般： 前端 tool 展示问题~~ ✅ 已完成

![tool截图](image.png)

1. ~~如图所示，tool 调用的内容，都居中了~~
   - 已修复：使用 `grid-cols-[80px_1fr_auto]` 布局替代 `flex`，确保三列（工具名、路径、状态）左对齐且结构稳定
   - 优化了 tool 列表的间距，添加 `space-y-0.5` 提升视觉层次

# 重要： 是否需要支持多个会话

1. 怎么创建多个会话
2. 后端server需要做哪些修改
3. 前端怎么兼容
4. session消息文件还是用jsonl存储吗？ 
5. session有没有一些结构化信息需要用数据库存储
6. /home/like/projects/bub /home/like/projects/republic  里面是怎么实现的