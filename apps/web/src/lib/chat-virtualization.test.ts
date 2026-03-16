import { describe, test } from "node:test"
import assert from "node:assert/strict"

import {
  buildMeasuredWindow,
  calculateAnchorScrollTop,
} from "./chat-virtualization"

describe("chat virtualization", () => {
  test("buildMeasuredWindow uses measured heights for spacers", () => {
    const window = buildMeasuredWindow({
      itemIds: ["a", "b", "c", "d", "e", "f"],
      containerHeight: 300,
      scrollTop: 260,
      measuredHeights: {
        a: 100,
        b: 120,
        c: 140,
        d: 160,
        e: 180,
        f: 200,
      },
      overscan: 1,
      minItems: 1,
      defaultItemHeight: 100,
    })

    assert.equal(window.startIndex, 1)
    assert.equal(window.endIndex, 5)
    assert.equal(window.topSpacerHeight, 100)
    assert.equal(window.bottomSpacerHeight, 200)
  })

  test("calculateAnchorScrollTop keeps anchor stable when height grows above viewport", () => {
    const nextScrollTop = calculateAnchorScrollTop({
      currentScrollTop: 500,
      currentOffset: 180,
      desiredOffset: 240,
      maxScrollTop: 5000,
    })

    assert.equal(nextScrollTop, 440)
  })

  test("calculateAnchorScrollTop clamps to valid bounds", () => {
    assert.equal(
      calculateAnchorScrollTop({
        currentScrollTop: 20,
        currentOffset: 200,
        desiredOffset: 400,
        maxScrollTop: 1000,
      }),
      0
    )

    assert.equal(
      calculateAnchorScrollTop({
        currentScrollTop: 900,
        currentOffset: 700,
        desiredOffset: 0,
        maxScrollTop: 1000,
      }),
      1000
    )
  })
})
