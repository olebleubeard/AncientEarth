# Full-frame composite solver: rejected

The single 1920×1080 comparison failed the unchanged performance gate. After
three warmups, five retained complete-chain samples had medians of
**2,443.0174 ms for the candidate** and **325.5038 ms for the original**: the
candidate took **7.50534 times as long**. Both the required 20% reduction and
separated-range conditions failed. The [frozen decision audit](../../artifacts/optical-composite-f64-full-candidate-first-independent-review.json)
and [independent raw closure](../../artifacts/optical-composite-f64-full-candidate-first-closure-review.json)
agree. This candidate is not adopted; production remains unchanged, with no
further tuning or rerun under this experiment.

Correctness checks passed before timing interpretation. All 2,073,600 launch
directions and final classifications matched the original. The candidate
completed 1,743,548 promoted sky suffixes, while 324,585 unhanded rays and
5,467 fallbacks retained all twelve original terminal words. The original's
35 unresolved classifications remained unchanged. Saved-state, ownership and
progress checks passed, as did the 23 actual-view sample checks, including all
27 frozen error comparisons for the nine accepted sample suffixes. The prior
branch, atmosphere and returning-ray controls also passed. These are the
[predeclared finite checks](cartesian-composite-full-decision.md), not a global
exact-ODE error bound for every sky pixel.

The measured interval includes initialization, legacy prefixes, promoted
continuation, discarded work, original-eye fallback and final validation.
It excludes compilation, readback, shading and presentation. Substantial
suffix coverage therefore did not produce a useful complete-chain gain.
These aggregate timings do not distinguish arithmetic precision, controller,
clearance or memory costs; they do not establish that f64 alone caused the
slowdown. No native moving-flight improvement or FPS result follows.
