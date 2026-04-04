const ANSI_OSC_PATTERN = /\u001B\][^\u0007\u001B]*(?:\u0007|\u001B\\)/g
const ANSI_CSI_PATTERN = /\u001B\[[0-?]*[ -/]*[@-~]/g
const ANSI_SINGLE_PATTERN = /\u001B[@-_]/g

export function stripAnsiSequences(value: string): string {
  return value
    .replace(ANSI_OSC_PATTERN, "")
    .replace(ANSI_CSI_PATTERN, "")
    .replace(ANSI_SINGLE_PATTERN, "")
}

