# F64 controller coverage failure and a separate correctness question

The first selected f64 experiment remains rejected under its frozen policy.
It required ten specified suffix completions and observed nine. This note
does not change that requirement, reinterpret the experiment as a pass, or
authorize a rerun or a controller change. It distinguishes the failed coverage
prerequisite from the final numerical results that were actually published.

The [frozen audit](../../artifacts/optical-cartesian-f64-selected-first-independent-review.json)
and [independent raw closure](../../artifacts/optical-cartesian-f64-selected-first-closure-review.json)
agree on the following result. Record 3, pixel `(110, 941)`, made 67 original
prefix attempts, two suffix attempts, and 752 attempts in the restarted original
solver. The suffix returned reason 3, which denotes rejection by the complete
clearance bridge. The retained data do not distinguish conversion/domain
failure, a charge exceeding tolerance, or geometric nonseparation. They do not
establish a physical intersection or identify a precision defect.

That record's twelve published terminal words equal the original solver's
words exactly. All 23 physical handoffs, original launches and prefix counts
are unchanged. The thirteen unhanded original records retain their complete
state, details and terminals. The nine published suffix results pass all 27
frozen position, direction and path comparisons. The frozen auditor's combined
`same_handoff_and_prefix` flag is false for record 3 because its comparison also
contains route word 22; the physical handoff words themselves are identical.
The other changed words for that record are the suffix/fallback counts and
failure reason. No published numerical regression was detected in this sample.

The independently reviewed source supplies the relevant safety distinction.
The original prefix and fallback retain their original arithmetic. A suffix
failure changes scheduling metadata and never publishes its tentative terminal.
Fallback starts at the stored original launch, not at the altered suffix state.
Consequently a conservative clearance rejection can reduce acceleration
coverage and increase work without changing the final original result. The
source and this record's byte parity support that statement; neither proves
that every future input will execute correctly. Complete input, branch,
fallback and numerical checks remain necessary.

There is therefore a scientifically defensible **separate prospective
question**: can the same unmodified controller accelerate a useful subset while
returning the exact original result for every declined ray? A required identity
of the accelerated subset is an engineering usefulness gate. It is not itself
a physical requirement when original-eye fallback remains exact. Reallocating
engineering effort after that gate fails does not justify changing any final
accuracy threshold or accepting an unresolved terminal.

If that separate effort is authorized, its policy must be frozen before the
additional candidate outputs. It must retain all of these obligations:

- Keep the current controller, field uploads, fixed quarter-tolerance,
  source/h0/scene guards and original-eye fallback unchanged. Do not tune
  admission or the solver to recover record 3.
- Preserve each original launch and saved handoff. Every unhanded or fallback
  result must match all twelve original terminal words; any new unresolved
  outcome or changed original class fails. Accepted suffixes must remain sky
  and pass every existing same-ray metric ceiling. The separate atmosphere
  fixtures retain their strict absolute limits for every final output.
- Record every route, reason and prefix/suffix/fallback count, including
  discarded work. A fallback is an original result, never evidence that the
  promoted solver passed that ray's accuracy test. Retain at least one genuine
  suffix completion; all-fallback output does not establish useful f64 solving.
- Complete the existing prospective branch/atmosphere tests and the separately
  frozen returning/turning test. An accuracy, identity, source or terminal gate
  failure stops the candidate without retuning. The existing selected
  coverage-rejection artifacts remain part of the evidence.
- Only after correctness closes, require the unchanged complete-chain
  performance rule: at least 20% median reduction with the prescribed separated
  sample ranges, including prefix, suffix, discarded work, original fallback,
  initialization and validation. A controller or suffix-only cost does not
  satisfy this rule. Native moving-view performance remains a further check.

This is not a demonstrated fast implementation, a global exact-ODE error proof,
or permission to relax model accuracy. The current evidence supports further
unchanged-source correctness evaluation if separately authorized. It supplies
no performance conclusion, and no candidate has been adopted in production.
