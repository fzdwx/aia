import { Shimmer } from "./shimmer"

interface TextShimmerProps {
  text: string
  active: boolean
  className?: string
}

export function TextShimmer({ text, active, className }: TextShimmerProps) {
  if (active) {
    return (
      <Shimmer as="span" className={className} duration={2}>
        {text}
      </Shimmer>
    )
  }

  return <span className={className}>{text}</span>
}
