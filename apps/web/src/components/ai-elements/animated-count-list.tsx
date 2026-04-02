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
    <motion.span layout className={className} data-slot="context-group-counts">
      <AnimatePresence initial={false}>
        {visibleItems.map((item, index) => (
          <motion.span
            layout
            key={item.key}
            initial={{ opacity: 0, width: 0, y: 4, scale: 0.98 }}
            animate={{ opacity: 1, width: "auto", y: 0, scale: 1 }}
            exit={{ opacity: 0, width: 0, y: -2, scale: 0.98 }}
            transition={{
              layout: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
              opacity: { duration: 0.16, ease: "easeOut" },
              width: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
              y: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
              scale: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
            }}
            style={{ overflow: "hidden", whiteSpace: "nowrap" }}
          >
            {index > 0 ? ", " : ""}
            <motion.span
              layout
              key={item.count}
              initial={{ scale: 1.22, color: "var(--text-strong)" }}
              animate={{ scale: 1, color: "var(--muted-foreground)" }}
              transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
              className="inline-block"
              style={{
                display: "inline-block",
                originX: "50%",
                originY: "50%",
              }}
            >
              {item.count}
            </motion.span>{" "}
            {item.count === 1 ? item.one : item.other}
          </motion.span>
        ))}
      </AnimatePresence>
    </motion.span>
  )
}
