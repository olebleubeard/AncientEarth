# Exact-state continuation diagnostic

This is an unregistered scheduling experiment for the existing midpoint scene
solver. It is not a new ray equation, an accepted replacement renderer, or a
performance result. The production numerical implementation remains unchanged.

The suspension point is immediately before an outer attempt of `trace` in
`src/desktop/shaders/refraction.wesl`. An attempt includes its complete local
error decision, reference-surface isolation, terrain and marker event query,
and any final root refinement. A dispatch may not suspend inside one of those
operations. In particular, a rejected trial is still an attempted iteration;
it preserves the same state and changes the proposed step exactly as the
original loop does.

Each ray has three `vec4<u32>` records, or 48 bytes:

| Record | Components |
| --- | --- |
| 0 | Position x/y/z and accumulated path, as binary32 bit patterns |
| 1 | Direction x/y/z and proposed step, as binary32 bit patterns |
| 2 | Entry path bit pattern, completed-attempt count, output-record ID, phase |

Phase zero means live and phase one means terminal. The existing terminal
record continues to carry position, direction, range, surface normal, material,
and owner. Immutable settings supply curvature and all weather/scene data.
The initial direction is needed only by initialization; it is already represented
by the current direction afterward. The production scene and coverage helpers
have no private state that persists between outer attempts.

Initialization retains the original readiness check and vacuum-entry algorithm.
An initialization return has zero completed attempts. Each subsequent attempt
increments the count once, including an attempt that rejects its trial or returns
a terminal result. After attempt 4,096, an otherwise live ray publishes the same
unresolved current state that the original loop publishes on exhaustion. No
4,097th attempt, restarted budget, or extra integration is permitted.

The first experiment uses fixed output-record indexing and bounded slices. A
selected-ray fixture maps that record ID through its immutable physical-pixel
table; a full-frame fixture uses the ordinary flattened pixel index. Optional
later compaction may change which invocation owns a ray, but must retain exactly
one live record per original pixel and scatter every terminal to that pixel.
No ray's numerical state may depend on another ray's scheduling order. All
settings, geometry, marker frames, and view inputs must belong to one unchanged
packet throughout the dispatch chain. A terminal packet must not be advertised
as current while any ray remains live. Host waits and readbacks are separate
from a GPU dispatch timing measurement.

Preserving expression text and storing floats through integer bit patterns does
not prove bit-identical GPU execution. Compiler reassociation, contraction,
normalization and subnormal behavior may differ after changing the surrounding
program. Acceptance therefore requires retained raw comparisons against the
original shader: every terminal word and every per-ray completed-attempt count,
including initialization returns, rejected trials, contacts, sky exits and
budget exhaustion. Numerical differences require independent accuracy review;
they are not accepted merely because the new schedule is faster.

The retained [legacy compiler report](../../artifacts/optical-gpu-tools/legacy-live-auto.stats)
lists 169 vector registers and 208 bytes of scratch per invocation. Continuation does not by itself remove the inner
scene-query graph or its scratch storage. It also adds state-buffer traffic and
dispatch overhead. Thus the useful question is measured reduction in inactive
lane work and complete-frame cost, not an assumed gain from the number of
dispatches. The existing full-frame midpoint terminal artifacts do not contain
outer-attempt counts; counters from a different solver cannot establish this
solver's compaction opportunity.
