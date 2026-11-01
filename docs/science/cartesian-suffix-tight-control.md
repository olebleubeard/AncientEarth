# One tighter-control Cartesian suffix experiment

This selected-only experiment is fixed before its GPU readback. It uses the
same 23 rays and physical field as the rejected first Cartesian suffix run.
It changes exactly one declaration in the suffix module:

```wgsl
let tolerance=0.25*max(0.002,4e-7*length(state.p));
```

The original prefix and fallback, exact handoff state, field/stage equations,
source ownership, boundary guards, top handling, scene exclusion and 4096
attempt limit remain unchanged. The existing controller applies this tighter
tolerance to its embedded-plus-interior acceptance, source-root allocation and
complete local scene-error check. The factor contracts the current budget;
it does not grant a new allowance. Accepted-step logging retains its existing
layout and expressions.

The preceding [telemetry audit](../../artifacts/optical-cartesian-suffix-trace-selected-first-independent-review.json)
verified every previous terminal, detail, state and progress byte and all 482
accepted-step records. At pixel `(320, 540)`, the exact rational sum of the
stored handoff path and accepted steps is `336401.2153778076 m`, with reference
error `0.19621927803382277 m`. The published `336401.25 m` has error
`0.16159708565101027 m`. The accumulated `+0.0346221923828125 m` rounding
therefore helped this ray. Replacing only this observed accumulation by its
exact sum cannot close its frozen path gate. That evidence does not separate
the remaining handoff, accepted-step,
field-evaluation and reference discrepancies.

Before interpreting this new run, independently verify identical five uploads,
prefix/fallback module bytes, launch words and exact handoff position,
direction, path, entry, geometry, source owner and prefix attempt count. The
same ten records (`3, 9, 10, 11, 12, 14, 15, 17, 18, 20`) must complete through
the suffix. All thirteen original-route terminals must remain bit-identical.
A new fallback cannot conceal a suffix failure. The accepted-step logs must
validate, including the final source-exit step and overflow checks.

All thirty endpoint/path/direction comparisons use the unchanged, prospectively
frozen same-ray oracle policy. No metric threshold is refitted. One failed
metric, lost suffix route, identity failure or invalid log rejects this
refinement branch before any full-frame timing. There is one factor, no factor
sweep and no automatic adoption. A passing selected result would still require
the previously declared branch matrix and complete-packet timing gates.

The result measures finite sensitivity to tighter local controls, including
the different f32 schedules and rounding they induce. Improvement would not
establish a truncation order, isolate every error source, or prove global
accuracy. Failure would not prove that every possible solver is inaccurate;
it closes this bounded refinement rather than authorizing further tuning.
