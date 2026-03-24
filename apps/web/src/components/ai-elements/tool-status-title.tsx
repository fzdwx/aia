import { TextShimmer } from "./text-shimmer"

interface ToolStatusTitleProps {
  active: boolean
  activeText: string
  doneText: string
  className?: string
}

export function ToolStatusTitle({
  active,
  activeText,
  doneText,
  className,
}: ToolStatusTitleProps) {
  if (active) {
    return <TextShimmer text={activeText} active className={className} />
  }

  return <span className={className}>{doneText}</span>
}
