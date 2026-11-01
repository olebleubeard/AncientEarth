# Cartesian atmospheric suffix: proposed bounded diagnostic

This is a design checkpoint, not an adopted solver or a performance result.
The proposed diagnostic preserves the original primary solver for every
near-scene contact. Its only candidate acceleration is the previously tested
direct-Cartesian Hermite DP atmospheric continuation. The canonical normal
chart, new Cartesian contact solver, shared-ray interpolation and changed
weather packing are outside this proposal.

## State and physical field

The legacy prefix keeps its original equations, step decisions, vacuum entry,
terrain coverage, contact ordering and terminal returns. A handoff is possible
only between completed accepted attempts, after the original terminal guards.
It copies the exact stored `State(p,u)`, accumulated path and atmospheric entry
path. No coordinates are transformed, no direction is normalized again, and
no path is reset. The suffix uses the same uploaded `geometry`/`qmul`, profile
nodes and gradients as the prefix. This avoids an initial coordinate
perturbation requiring a separate propagation argument.

The suffix retains the guarded direct-Cartesian Hermite implementation's
physical-boundary schedule, finite stages, embedded plus interior estimator,
both source-boundary tests, h0 boundary test, source-root allocation and
original polynomial parameter. It does not substitute the distinct
`physical=false` schedule. Initial source ownership must match `index_gradient`
for the exact stored `geometry(p).w` and the original packed knots, strictly
inside one interval by the retained finite selection band. This is a guard on
the same finite field evaluation, not a proof of exact geodetic height. A node does not
receive a guessed departure flag. The legacy prefix continues until this
condition holds or it terminates. Existing source ownership and departure
state are then preserved by the suffix's reviewed event controller.

The local metre budget remains `max(0.002, 4e-7*length(p))`, with `p` measured
from the original eye. The suffix starts with at most the remaining number of
the original 4096 outer attempts. Its existing finite inner iteration limits
and two-million-metre in-atmosphere path limit remain unchanged.

## Sufficient scene exclusion

The aggregate terrain-root upper plane is too loose in the retained packet:
it corresponds to roughly 5.4 km above the eye. The proposed test instead uses
all 11 existing per-patch root AABBs and the existing marker/sign root AABB.
This is an exclusion test only; it cannot accept a contact.

For an original dense displacement

`P(t) = p + t[d + (1-t)(r3 + t(r4 + (1-t)r5))]`,

the power coefficients after the constant are
`c1=d+r3`, `c2=r4-r3+r5`, `c3=-r4-2*r5`, `c4=r5`.
On a committed prefix `0 <= t <= alpha`, multiply these coefficients by
`alpha`, `alpha^2`, `alpha^3`, `alpha^4`, respectively, before the usual
degree-four Bernstein conversion. The five Bernstein controls are

`p`, `p+c1/4`, `p+c1/2+c2/6`,
`p+3*c1/4+c2/2+c3/4`, `p+c1+c2+c3+c4`.

Here the displayed `ci` in the control formula denote the scaled coefficients.
All operations, encoded affine frame transforms and interval extrema are
evaluated outward. Convex-hull containment encloses the whole original dense
prefix. Source-root clipping preserves `alpha=accepted_distance/original_step`
and includes the quotient's finite uncertainty; it must not rescale the
stored polynomial by ordinary rounded endpoint subtraction.

Every terrain patch must have at least one strictly separating axis between
this complete interval box and its root AABB, after retaining the existing
source-coordinate, transform, numerical and polynomial allowances. The
marker/sign root needs the same whole-prefix exclusion in eye coordinates.
The padding cannot be smaller than the corresponding original broadphase
padding. The metre tube is multiplied by an outward upper bound on each
uploaded transform row's L1 norm; rounded rows are not assumed orthonormal.
This is separate from outward polynomial/affine evaluation and source padding.
Invalid metadata, arithmetic, bounds, a nonseparating patch or any possible
h0 contact/crossing declines the suffix. Empty roots are skipped only when
the existing packet explicitly represents an empty scene.

The prefix endpoint check is merely an eligibility hint. Every suffix step,
including its final atmospheric-exit prefix, must pass the whole-prefix
certificate before state or terminal publication. A descending or turning
curve receives no permanent sky exemption. A failed scene certificate requests
fallback immediately, rather than creating a new near-boundary step-halving
policy.

The isolated helper retains the larger of the original physical-curvature
sag and the prior Hermite graph's polynomial sag. The Bernstein enclosure,
not that retained sag, covers the small parameter extension from outward
division. Its dense-evaluation allowance is `64*2^-23*max(1,M)`, where `M`
is an outward L1 sum of the stored position and four displacement vectors.
For the admitted encoded parameter upper bound `f32(1.000002)` and `M<=1e9`,
at most twelve basic-operation roundings per represented term give
`gamma12*A^4*M`, with `gamma12=12*u/(1-12*u)` and `u=2^-23`.
The remaining margin covers the explicitly bounded flush-to-zero terms;
the exact-rational test checks this inequality. Source-root uncertainty is
in path metres, so the entire supplied error is conservatively multiplied
by at least the outward dense physical-speed bound before frame row gains.
These allowances make exclusion harder; they do not change local ODE/contact
acceptance budgets or authorize a surface hit.

## Completion and fallback

Four ordered stages are proposed: legacy prefix, atmospheric suffix, original
full trace for flagged pixels, then validation/header publication. A suffix
failure discards all candidate progress and reruns the unchanged original
trace from the stored original launch. Its reason and extra work are retained
in diagnostics. It never resumes legacy integration from a modified DP state,
publishes a partial sky terminal, downgrades a terminal, or reuses an earlier
frame. This restart is explicit and its cost belongs in total timing.

The fixture interface uses the existing scene packet and terminal layout,
separate checked shader modules, fixture-declared dispatch steps, a provisional
24-word per-ray state record and progress `[0, record_count, ...]` only after
all records are complete. The state allocation is about 199 MB at 1920×1080;
this diagnostic ABI is not a native-memory-readiness claim.

## Gates before a performance claim

First freeze the design and independently review the stored source-owner guard,
whole-polynomial exclusion and unchanged original fallback. One bounded
selected run must include actual successful handoffs and unchanged legacy
terrain/marker outcomes. Retain prefix, suffix and fallback counts, masks and
raw states. A zero-handoff result rejects this route before full-frame timing.

The correctness matrix must retain sky, horizon, terrain, prism/sign, inversion,
turning, vacuum-entry/exit, invalid aperture and not-ready cases. Fallback and
unhanded terminals must match the counted original reference exactly. Candidate
sky results need independent tightened source-aware references, with reference
convergence reported separately. The existing atmosphere fixture's limits
(5 m endpoint/path and 3e-6 direction-vector distance) retain their original
fixture scope. The separately frozen actual selected baseline exceeds these
limits on some original sky rays; that failure remains reported. It cannot
simultaneously be required to match unchanged fallback bits and satisfy a new
absolute threshold that those same original bits fail.
Before any candidate readback, the selected matrix instead requires each
accepted suffix sky ray to have no larger endpoint, path or direction error
than its same original ray, allowing only that ray's independently measured
reference-convergence difference. The initial aggregate contract is retained
alongside the prospective scoped amendment; no candidate errors informed it.
These are finite-test gates, not a global image accuracy theorem. They permit numerical-method differences
without treating arbitrary bit identity as a physical accuracy definition.

Only after these gates pass should one full-packet matched timing include all
prefix, suffix, fallback, validation and buffer traffic. A median reduction of
at least 20%, with the retained five-sample ranges separated, is the existing
engineering effort gate, not a significance test. If it fails, stop this
variant without a parameter sweep. Passing it may justify native integration;
it does not satisfy the separate moving-flight target of at least 30 FPS in
each demanding native phase. The previously measured roughly 80 ms Cartesian
candidate already prevents promising that target from this step alone.

## First selected result

The [first selected GPU run](../../artifacts/optical-cartesian-suffix-selected-first.json)
completed ten sky suffixes and preserved all twelve terminal words for the
thirteen rays that remained in the original solver. It did not exercise a
fallback restart. The [runtime audit](../../artifacts/optical-cartesian-suffix-selected-first-runtime-review.json)
verified the unchanged uploads, launch words, route/state records and completion
header; it is not an accuracy or throughput result.

The [independent numerical audit](../../artifacts/optical-cartesian-suffix-selected-first-independent-review.json)
rejected the predeclared per-ray gate. At pixel `(320, 540)`, path error rose
from `0.13034708565101027 m` to `0.16159708565101027 m`, exactly `0.03125 m`.
All other 29 tested sky error components passed. No threshold was changed,
and no full-frame timing or production adoption followed. The terminal
values alone cannot distinguish path accumulation roundoff from the accepted
steps' physical/reference error.

The subsequent [accepted-step audit](../../artifacts/optical-cartesian-suffix-trace-selected-first-independent-review.json)
verified exact terminal/detail/state/progress parity and all 482 log records.
For the failed ray, exact summation of the stored handoff path and accepted
steps increases reference error to `0.19621927803382277 m`: the accumulated
`+0.0346221923828125 m` rounding helped its published result. Replacing only
this observed accumulation by its exact sum cannot close the gate. The
remaining discrepancy is not further attributed.
A separate [one-factor tighter-control check](cartesian-suffix-tight-control.md)
retains the same per-ray gates; neither telemetry nor its diagnosis authorizes
production adoption or a performance claim.
