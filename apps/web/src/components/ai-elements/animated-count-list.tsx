import { AnimatePresence, motion } from "motion/react"

export interface CountItem {
  key: string
  count: number
  one: string
  other: string
}

interface AnimatedCountListProps {
  items: CountItem[]
  fallback?: string
  className?: string
}

function formatCountItem(item: CountItem): string {
  return `${item.count} ${item.count === 1 ? item.one : item.other}`
}

export function AnimatedCountList({
  items,
  fallback,
  className,
}: AnimatedCountListProps) {
  const visibleItems = items.filter((item) => item.count > 0)

  if (visibleItems.length === 0 && fallback) {
    return <span className={className}>{fallback}</span>
  }

  if (visibleItems.length === 0) return null

  return (
    <span className={className} data-slot="context-group-counts">
      <AnimatePresence initial={false}>
        {visibleItems.map((item, index) => (
          <motion.span
            key={item.key}
            initial={{ opacity: 0, width: 0 }}
            animate={{ opacity: 1, width: "auto" }}
            exit={{ opacity: 0, width: 0 }}
            transition={{ duration: 0.2, ease: "easeOut" }}
            style={{  overflow: "hidden", whiteSpace: "nowrap" }}
            className='mt-[2px]'
          >
            {index > 0 ? ", " : ""}
            {formatCountItem(item)}
          </motion.span>
        ))}
      </AnimatePresence>
    </span>
  )
}
