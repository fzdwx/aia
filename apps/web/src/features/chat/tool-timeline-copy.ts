export const toolTimelineCopy = {
  groupStatus: {
    running: "Exploring",
    completed: "Explored",
  },
  summaryCategory: {
    read: "read",
    search: "search",
    list: "list",
  },
  contextCount: {
    read: { one: "read", other: "reads" },
    search: { one: "search", other: "searches" },
    list: { one: "list", other: "lists" },
  },
  action: {
    expand: "Expand",
    collapse: "Collapse",
    copy: "Copy",
    copied: "Copied",
  },
  section: {
    request: "Input",
    result: "Result",
    content: "Content",
    failure: "Failure",
    topResult: "Top Result",
    issue: "Issue",
    issueIgnored: "Issue Ignored",
    patch: "Patch",
    rawDetails: "Raw Details",
  },
  toolName: {
    read: "Read",
    list: "List",
    search: "Search",
    patch: "Patch",
    shell: "Shell",
  },
  unit: {
    line: "lines",
    field: "fields",
  },
  searchResult: {
    codeMatch: "Code match",
    webResult: "Web result",
    openResult: "Open result",
  },
} as const

export type ToolTimelineCopy = typeof toolTimelineCopy
