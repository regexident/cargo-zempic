# `profile/opt-level`

The `opt-level` setting chooses how hard the compiler optimizes. It accepts `0`
to `3` and the strings `"s"` (optimize for size) and `"z"` (optimize for size
without loop vectorization).

The release default is `3`, which targets speed. When the goal is a small
artifact, `"z"` asks the compiler to prefer size over speed, disabling loop
vectorization that tends to enlarge code. `"s"` is the middle ground: size
optimization while keeping vectorization.

## Tradeoff

Optimizing for size can make compute-heavy code slower. `"z"` is the stronger
choice and the more pronounced slowdown; `"s"` keeps more of the speed. A
latency-critical binary should stay on `3` and look for size elsewhere, or
apply `"z"` only to the code that is not on the hot path.

## Dimension

This rule is dimensioned, because the recommendation differs by optimization
objective. The size objective is identified as `profile/opt-level:size`. A speed
objective would recommend the opposite value and carries its own identifier.
