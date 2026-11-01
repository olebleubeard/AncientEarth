# Refraction optimization working checkpoint

Updated 2026-10-26. **The bounded complete-frame experiment rejected the
candidate. Production remains unchanged.** The correctness matrix and frozen
full-frame accuracy checks passed, but the complete compute sequence was
substantially slower than the original. Independent final raw-decision review
confirmed the rejection; no further candidate variants or performance runs are planned
in this bounded investigation. The user specifically raised token consumption
and lack of a demonstrated moving-view speedup.

The frozen decision is retained at
`artifacts/optical-composite-f64-full-candidate-first-independent-review.json`:
`correctness_passed: true`, `passed: false`, `decision: reject-without-tuning`.
Its independent closure is retained at
`artifacts/optical-composite-f64-full-candidate-first-closure-review.json`.
The mathematics lead reports medians of 2443.02 ms for the candidate and
325.50 ms for the original, about 7.5 times slower. These are complete GPU
compute-chain timings, not application FPS. Aggregate timings do not isolate
which candidate component caused the extra cost. No native integration or
moving-flight acceptance follows this failed gate. Refraction remains off by
default; smooth flight with refraction enabled is still unresolved.

Scientific interpretation and limitations are recorded separately in
`docs/science/cartesian-composite-full-outcome.md`. Resume from this decision,
not the historical pending steps below. Do not replay or tune this rejected
candidate merely to obtain a different result.

The exact paired source evidence is preserved in
`artifacts/optical-composite-f64-full-pair-first-source.zip` and its
`-source-archive.json` dependency mapping. The first offline full-frame compile
failed on a host JSON-macro recursion limit; its original host is preserved.
Moving the added metadata assignment outside that macro fixed compilation;
the second offline validation passed without numerical changes.

The returning-original source archive is now complete and verified at
`artifacts/optical-composite-f64-returning-reference-first-source.zip` with
its `-source-archive.json` mapping. All nine approved candidate cases completed
successfully and passed their frozen per-case auditors and aggregate independent
raw-result closure. Their unchanged inputs are preserved through
`artifacts/optical-composite-f64-matrix-first-shared-source-archive.json`;
each run's exact executable is retained separately. The returning original's
independent reference closure has passed. Its candidate also passed both the
prospective auditor and independent raw-result closure, retained at
`artifacts/optical-composite-f64-returning-first-closure-review.json`.
The full correctness matrix is closed. The renderer mathematics agent authored
the minimal complete-frame adapter; the lead authored the frozen result
policy/auditor, with independent source review. Root added mechanical
host/wrapper routing and checked invalid flag combinations. The one matched
original and candidate complete-frame pair completed successfully and produced
the rejected performance result above.

State at the last pause (historical snapshot):

- The production solver and app build remain unchanged during this resumed
  investigation. Refraction defaults off. Smooth moving-view performance has
  not been demonstrated; there is no new full-frame timing or native FPS result.
- The separate composite atmosphere GPU case passed independent raw closure.
  The retained selected sample passed separately labeled final-output checks;
  its historical required-route rejection remains unchanged.
- Nine remaining cases passed offline compilation, assembled-source and
  prospective runtime-auditor reviews. Their candidate GPU runs have **not**
  occurred. The returning candidate GPU run has **not** occurred.
- Corrected returning-original offline validation passed after an independently
  reviewed decimal-ingestion fix; the failed first attempt is preserved.
  The original GPU run `artifacts/optical-composite-f64-returning-reference-first`
  completed exit zero with stable sources/executable and no timeout.
- The reviewed original/reference freezer completed successfully under
  `artifacts/optical-composite-f64-returning-baseline-freeze-first`, including
  its `.json`, `-contract.json` and lifecycle/log files. Independent final
  reference-result closure was interrupted and is pending. This is the original
  control, not acceptance of the accelerated returning candidate.
- The current composite input inventory has 776 paths and was frozen through
  the returning original/freezer. **Archive the original run's source inputs
  before changing inventoried files or appending the reference contract.** Use
  `-source.zip` / `-source-archive.json` with the agreed single-run manifest,
  path/blob mapping, complete disjoint coverage and explicit separately retained
  prior ZIP/executable/library dependencies. This archive is not yet made.
- Latest lossless storage report is
  `artifacts/optical-composite-storage-preparation-third.json`: all 57 processed
  files retained their original lengths and hashes; no files were deleted or
  moved. Available space was approximately 2.15 GB after completion.

On an explicitly requested resume, first reassess the remaining effort against
the user's concern about token consumption. The investigation has produced
accuracy evidence, but no demonstrated moving-flight speedup. Do not repeat
closed experiments or claim the optimization is finished.

The sections below retain the detailed pre-pause working state and history.

Current step: complete the separately authorized composite-solver correctness
matrix after the first atmosphere packet passed its frozen auditor.
The first isolated f64 suffix experiment remains rejected by its frozen
required-route gate. Independent raw-result review distinguished that
acceleration-coverage failure from a published numerical error: the declined
ray returned the exact original result. Root authorized a new prospective
policy with unchanged numerical shaders and final accuracy gates, exact
original fallback, explicit route accounting and required exercised paths.
The complete-chain speed gate remains unchanged. This reallocates engineering
effort; it does not retroactively pass the earlier contract or claim a speedup.
No tuning, timing or production adoption is authorized. Each remaining case
needs the new policy, source adapter and auditor independently reviewed before
execution; returning rays additionally need a frozen original control and
independently converged reference before the candidate.

The distinct host mode is `-CartesianCompositeF64 CASE`, with
`-CartesianCompositeReference` permitted only for `returning-gradient`.
Full-frame/timing and mixed fixture options are blocked. Five invalid command
combinations were checked to reject before compilation or artifact creation.
The first atmosphere offline check passed under
`artifacts/optical-composite-f64-atmosphere-validation-first`, with stable
inputs and no graphics-device calls. The independent source/policy gate is
retained at `artifacts/optical-composite-f64-initial-source-review-first.json`.
The separate runtime auditor passed independent review at
`artifacts/optical-composite-f64-runtime-source-review-first.json`.
`artifacts/optical-composite-f64-selected-reuse-first-review.json` retains the
separately labeled selected-data assessment without rerunning the selected
packet or changing its historical rejection.

The first new GPU run, `artifacts/optical-composite-f64-atmosphere-first`,
completed with stable inputs/executable, exit zero and no timeout. Its frozen
`-independent-review.json` passes the atmosphere accuracy checks. Independent
raw-result closure passed in `-closure-review.json` and was cross-reviewed.
Its exact source/data archive is retained
in `-source.zip` with the verified `-source-archive.json` mapping. Root
authorized preparing the remaining correctness cases while this raw closure
finishes; no further GPU execution follows until the required reviews close.
This is not a performance or production acceptance result.

Nine remaining existing cases passed offline validation under
`artifacts/optical-composite-f64-matrix-CASE-validation-first`. The batch then
stopped before GPU execution while constructing the new returning original:
`artifacts/optical-composite-f64-returning-reference-validation-first` exited
101 on a retained-direction equality assertion. The returning candidate was
not run. Mathematics reviewers identified decimal ingestion as the cause and
are reviewing a parsing-only correction; the numerical shader must remain
unchanged. The successful and failed attempts share an exact verified archive
at `artifacts/optical-composite-f64-matrix-offline-first-source.zip`, with each
run's source manifest and a reconstruction map in the companion archive report.
The remaining-matrix auditor and returning reference freezer are separate new
files; the approved initial composite auditor is preserved unchanged.

Closed certificate/geometry evidence received lossless NTFS compression to
make room for new captures. `artifacts/optical-composite-storage-preparation-first.json`
records every original output-manifest hash and verifies unchanged content and
length after compression. No evidence was deleted or moved.

The first correctness gate consists of two selected, untimed cases:
`selected` and `atmosphere-100-gradient`, using the existing frozen inputs and
references. The new host option is `-CartesianSuffixF64 CASE` (CLI
`--cartesian-suffix-f64 CASE`). Full-view timing and other candidate/reference
modes are rejected for this option. Root has added the mechanical host path
and f64/int64 device features. Both initial cases passed offline compilation,
WGSL/module/binding validation, with stable inputs and no GPU execution:
`artifacts/optical-cartesian-f64-selected-validation-first` and
`artifacts/optical-cartesian-f64-atmosphere-validation-first`. Independent
bridge/adapter/assembled-source and numerical-auditor gates closed. The selected
GPU run completed with stable inputs/executable and no timeout under
`artifacts/optical-cartesian-f64-selected-first`. Its frozen
`-independent-review.json` records the rejection; `-closure-review.json`
independently confirms it and distinguishes physical handoff from route
identity. The exact source/data inputs are retained in `-source.zip` as
deduplicated lossless blobs with a `paths.json` reconstruction map. The
`-source-archive.json` records verified hashes and explicit exclusions for
compiled libraries and separately retained executables. The host/wrapper
pre-edit snapshot remains under `artifacts/optical-cartesian-f64-host-before`.

Ownership: the rendering mathematics agent owns
`scripts/optical_cartesian_suffix_f64_fixture.rs` and its input inventory;
the lead owns the outward clearance bridge, proof and tests; the independent
reviewer checks both. The reviewed design is
`docs/science/cartesian-f64-suffix-design.md`. The legacy prefix/fallback,
fixed numerical budget, all frozen full-ray/matrix accuracy gates and the
complete-chain speed gate remain required. No accuracy failure permits
parameter tuning. Production adoption is not authorized by a component test.

Latest completed evidence:

- `artifacts/optical-cartesian-direct-gpu-first`: every actual recorded numeric
  output passed the frozen and independent audits. The full-store workload
  failed its fixed component cost gate. The verdict remains rejected, with no
  rerun or threshold change. Exact source/data inputs, executable, all raw
  buffers and the independent closure are retained. The raw buffers received
  lossless NTFS compression only after timing and auditing; all logical lengths
  and original SHA256 hashes were verified unchanged. No evidence was removed.
- `artifacts/optical-cartesian-trial-gpu-first`: the preceding checksum workload
  passed its separate detailed arithmetic checks but failed detail/cost
  consistency. Reduced hashes cannot bound the differing numeric values. Its
  timings remain uninterpreted and the workload remains rejected.
- Both CPU handoff feasibility screens passed independent review, including
  the actual uploaded field. Their prefixes are
  `artifacts/optical-handoff-feasibility-first` and
  `artifacts/optical-handoff-packed-field-first`. They do not establish GPU
  controller accuracy or performance.
- The earlier tighter f32 Cartesian suffix failed the
  `atmosphere-100-gradient` matrix gate. Both 5000-height candidate cases remain
  unrun. The original-solver clearance/fallback split passed full-frame bit
  parity but failed its speed gate. Both routes remain closed.

**Moving-flight performance is not solved.** The app still uses the original
solver and defaults refraction off. Per-run executables are retained for new
evidence runs; older overwritten binaries cannot be recovered from hashes.

## User requirements

- Refraction stays disabled by default, with its existing GUI toggle.
- Improve flight performance when refraction is enabled without weakening the
  physical model or numerical acceptance checks.
- Mathematics must be authored by a mathematics subagent and independently
  reviewed. Preserve source data, license notices and all failed evidence.

## Production state

The production numerical solver remains unchanged. Accepted engineering work
includes same-frame primary dispatch, shared stellar calculations and exact
reuse of stationary primary results. The reuse key excludes only the output
generation token; changed numerical inputs, viewport, immutable buffer
identities or pipeline identity require a fresh solve. The UI explicitly
explains the default-off refraction option.

The built executable is `target/debug/ancient-earth.exe`. It includes the
stationary cache and diagnostic capture fixes. Experimental solvers below
are isolated scripts and are not enabled in ordinary runs.

Desktop tests, workspace/all-target Clippy, formatting, asset licensing and
software/font notice checks passed for this production revision. See
`artifacts/refraction-reuse-desktop-tests.log` and
`artifacts/refraction-reuse-clippy.log`.

**Smooth moving-view refraction has not been achieved.** Exact stationary
reuse does not accelerate changed views. Native captures with unavailable
desktop focus are diagnostics, not qualified foreground FPS measurements.

## Preserve these decisions

- Midpoint field-reuse and carried-geometry variants were rejected after
  independent reference checks found accuracy regressions. Do not enable them.
- Whole-attempt continuation and its reduced-bookkeeping variant reproduced
  complete terminal results exactly but were slower than the original solver.
  Do not enable them or continue bookkeeping sweeps without new evidence.
  See `docs/science/optical-wavefront-review.md` and the
  `artifacts/optical-wavefront-*` reports.
- Canonical-coordinate and certified-piece scene work remains experimental.
  Tests distinguish individual interval/contact results from complete rays.
- A sampled interpolation error is not a whole-cell admission certificate.
  No shared correction cell has been approved for production.

## Experiment history

The following entries are chronological. References to a next step within an
older entry describe that stage of the investigation; use the current step at
the top of this file when resuming work. Preserve the closed rejections.

The lead mathematics agent is developing a cheaper representation that factors
geometric travel out of the refractive correction. Its first CPU rejection and
cost screen is `scripts/optical_deviation_stencil.rs`, run by
`scripts/verify_optical_deviation_stencil.ps1`. Retained results are
`artifacts/optical-deviation-stencil-first.*`.

A telemetry-only repeat is retained as
`artifacts/optical-deviation-stencil-terminal-first.*`; it records termination
reasons without intentionally changing decisions. Compare the earlier state
bits and non-timing fields before interpreting it. The cheap second-variation
foundation is in `scripts/optical_second_variation.rs`; it does not itself
certify a ray or admit a shared cell.

The endpoint correction is retained in
`artifacts/optical-deviation-stencil-progress-first.*`. It integrates the
remaining representable interval instead of stopping at the former minimum
step cutoff. The independent audit retained all previous source states and
successful results exactly, and checked the newly completed endpoint steps.
See `artifacts/optical-deviation-stencil-progress-first-independent-review.json`.
This screen still does not admit a shared cell.

Next: develop and independently review a cheap sufficient bound for a complete
cell. The current CPU prototype is `scripts/optical_deviation_flow.rs` and its
separate bounds modules. CPU timings compare
different numerical policies and do not predict GPU speed. Scene safety,
finite baseline evaluation and between-source-height errors remain separate
requirements. The existing individual-ray path must remain the fallback.
The same CPU wrapper exposes this separate screen with `-Flow`; it records
the selected source set and uses a separate executable. Run it only after
source review and while no graphics compilation or benchmark is active.
The first reviewed-source run completed at
`artifacts/optical-deviation-flow-first.*`. The lead rejected this producer
on both cost and bound tightness. Independent evidence audit agreed; see
`artifacts/optical-deviation-flow-first-independent-review.json`.
Do not port it to the GPU or continue parameter sweeps. The next priority is
complete accepted-curve/scene integration using the canonical-coordinate
solver, followed by real-ray correctness and full-view timing checks.

The rendering mathematics agent is diagnosing the remaining canonical contact
uncertainties. `artifacts/optical-positive-contact-gpu-first-reviewed-scope.json`
is the corrected independent scope report. It validates nominal ownership,
contact brackets and path results for the successful component cases; it does
not independently validate every returned state component or complete rays.

The new positive-case return-site diagnostic is exposed as
`-PositiveStages` by `scripts/verify_optical_normal_scene.ps1`. Offline
validation passed under `artifacts/optical-positive-stage-validation-first.*`.
The GPU repeat `artifacts/optical-positive-stage-gpu-first.*` also passed the
independent original-word parity audit. Its tags identify later-face membership
uncertainty in the long-prism case. The rendering mathematics agent is reviewing
a narrowly scoped chronological-pruning experiment using certified bounds.
The reviewed helper is exposed through `-PositiveChronology`; offline
validation passed under `artifacts/optical-positive-chronology-validation-first.*`.
The bounded GPU replay also completed. Its independent audit is
`artifacts/optical-positive-chronology-gpu-first-independent-review.json`:
all six inputs were unchanged, the long-prism case now resolves to the first
entry, and the other five cases retained their original outputs. The shared
edge remains explicitly uncertain. These are manufactured contact checks,
not a complete physical-ray or native performance acceptance.
The new selected-ray pipeline is being prepared in
`scripts/optical_normal_selected_fixture.rs` with its own source inventory.
The mechanical host is `scripts/optical_normal_selected_probe.rs` and wrapper
`scripts/verify_optical_normal_selected.ps1`. These keep producer and scene
consumer shaders separate, preserve all continuation and generated-piece
buffers, and do not yet provide a full-view or timing mode. Host compilation
and both-module offline validation passed under
`artifacts/optical-normal-selected-validation-second.*`. The first failed
validation is retained; a missing shader local was fixed by the mathematics
author. Independent continuation review then closed after the finite-policy
fixes, and current-source offline validation passed again under
`artifacts/optical-normal-selected-validation-third.*`. The bounded GPU run
`optical-normal-selected-gpu-first` hit its 600-second watchdog while creating
the producer pipeline, before dispatch. No ray outputs or timing results were
produced; the owned process was terminated and all logs/fingerprints retained.
Whole accepted curves proven above
the scene already advance internally. Mixed scene-sensitive trials whose
individual pieces are subsequently excluded can still return to a later
dispatch for commit. It is a correctness diagnostic, not a throughput
candidate.

The isolated `-SplitCertificate` variant separates certificate generation
into a third module and inserts its dispatch between producer and scene
consumer. The original mode is retained. All three modules passed offline
validation under `artifacts/optical-normal-selected-split-validation-first.*`.
Independent source review approved the split before the bounded GPU retry
`optical-normal-selected-split-gpu-first`. All five pipelines compiled and
the run completed with unchanged inputs. All 23 records returned status 2,
so process completion is not numerical acceptance. The mathematics agents
are auditing the retained continuation and terminal buffers before selecting
the next change. This diagnostic has no timing mode and provides no native
flight-performance result. Buffer sizes, round limits and numerical equations
remain owned by the mathematics fixture. The mechanical wrapper now saves
compiler input fingerprints and its log before compilation, so failed builds
also retain their provenance.

The selected continuation audit and cross-review now pass seven test groups,
including the split dispatch order. The returned-ray audit still rejects
adoption. Its independent raw audit and separate suspected-contact reference
review are retained under the split GPU prefix and were cross-reviewed. The
contact discrepancy does not establish a missed canonical face, but those
reference checks do not approve the candidate's unpublished trajectory.

The next bounded experiment changed only compiler-added loop bounding on the
original reference shader. Mathematics authored and independently reviewed
the exact-source termination guard, including four Rust and six Python test
groups. Offline validation passed at
`artifacts/optical-primary-finite-loops-validation-first.*`. The matched pair
`optical-primary-loop-control-checked-first` and
`optical-primary-loop-control-finite-first` completed with stable inputs and
identical full output. Independent review rejected the performance gain as
immaterial under the predeclared gate. Do not sweep this branch or integrate
it into production. The opt-in diagnostic remains restricted to the archived
full-frame fixture; production retains all compiler runtime checks. See
`docs/science/primary-loop-termination.md` for the proof and decision policy,
and `artifacts/optical-primary-loop-control-finite-first-independent-review.json`
for the complete input/output and timestamp audit.

The bounded CPU patch/face-relative feasibility screen completed under
`artifacts/optical-patch-relative-fit-first.*`. Its numerical sources were
authored and independently reviewed by the mathematics agents. Both the
independent numerical audit and the separate rendering comparison reject
this construction: it admits no additional retained curves. Do not develop
a contact framework or GPU port for this variant. The reports distinguish
endpoint checks from curve admission and from complete contact handling;
the raw report's generic "some admission" wording does not override the
independent rejection. See the same prefix's `-independent-review.json`
and `-rendering-comparison.json` files for the exact comparison and scope.

The mechanical runner is `scripts/verify_optical_patch_relative.ps1`, with
`scripts/optical_patch_relative_feasibility.py` and its math-owned complete
`scripts/optical_patch_relative_inputs.txt`. The run exited successfully
with unchanged inputs. It records the interpreter and selected numerical
module fingerprints, full supplied local inputs, bounded owned-process
lifecycle and before/after hashes. Those runtime fingerprints do not audit
the whole Python installation. No new Python dependency was installed.

Current investigation: the mathematics lead has implemented a transition from
the existing near-scene solver into the experimental Cartesian atmospheric
solver only after a justified scene-clear transition. It would retain the
accepted Cartesian state and uploaded field. Review identified additional
unresolved error-propagation requirements for the alternative canonical
coordinate bridge, so that alternative is not being implemented. The rendering
agent independently reviewed the Cartesian proposal and clearance helper;
the lead reviewed the rendering agent's fixture. This is an isolated
diagnostic, not an accepted production transition. Earlier atmosphere-only
timings do not establish smooth flight. Preserve near-scene contacts, source-layer ownership, re-entry
handling and existing error budgets. A decisive correctness and cost screen
must precede any production integration.

The existing continuation wrapper now also retains compiler inputs and logs
before compilation, including failed attempts. Its PowerShell parser check
passed. The mechanical host/wrapper additionally support `-CartesianSuffix`
with separate checked shader modules and fixture-declared state stride and
dispatch order. Both independent reviewers have been asked to review the
new fixture and reference evidence; the rendering reviewer approved the
design for implementation and reviewed the mechanical host. The scientific
contract is `docs/science/cartesian-suffix-design.md`. The concrete fixture,
clearance helper, focused tests and complete input inventory now exist.
The first two offline validations failed on WGSL syntax; both are retained
under `artifacts/optical-cartesian-suffix-validation-first.*` and
`artifacts/optical-cartesian-suffix-validation-second.*`. The math author
corrected the reserved identifier and invalid field accessor. The third
offline validation compiled the host and validated all assembled modules
successfully under `artifacts/optical-cartesian-suffix-validation-third.*`.
This validation made no GPU calls. The independent assembled-source review
then passed. The bounded selected GPU run completed successfully under
`artifacts/optical-cartesian-suffix-selected-first.*`, retaining all buffers
and unchanged input fingerprints. The rendering review's initial raw audit
found successful suffix use for all original sky records and exact original
terminals for all other selected records. The independent per-ray reference
comparison rejected this candidate: one sky record worsens path error under
the frozen per-ray gate, despite improving its position and direction.
The other sky records pass. The `-runtime-review.json` and
`-independent-review.json` reports under that prefix are complete and
cross-reviewed. Original source files are preserved in the `-source.zip`
archive with verified path/hash mapping in `-source-archive.json`.
No threshold change, full-frame timing or production integration is approved.
The application renderer remains unchanged.

The next bounded step is accepted-step logging to diagnose the path error,
not a corrected solver. The math-owned wrapper is
`scripts/optical_cartesian_suffix_trace_fixture.rs`, with its own input
inventory. Root added `-CartesianSuffixTrace` to the existing mechanical
host/wrapper: selected records only, no timing, separate raw auxiliary buffer.
The independent source/host review and offline validation passed under
`artifacts/optical-cartesian-suffix-trace-validation-first.*`. The assembled
strip-back review passed, followed by the bounded replay under
`artifacts/optical-cartesian-suffix-trace-selected-first.*`. The initial
independent audit found exact old terminal, state and progress outputs.
Its accepted-step sum shows that removing accumulation error alone would
worsen the failing ray, so a compensated-path-only correction is rejected.
The full trace audit is retained in the same prefix's
`-independent-review.json` and has been cross-reviewed.

The lead authored one bounded tighter-control check, independently approved
by both reviewers. Its fixture is
`scripts/optical_cartesian_suffix_tight_fixture.rs`; its prospective contract
is `docs/science/cartesian-suffix-tight-control.md`. Root added the
`-CartesianSuffixTight` flag using the existing selected-only trace interface.
Offline validation passed under
`artifacts/optical-cartesian-suffix-tight-validation-first.*`. Independent
assembled-source reversal passed, followed by the bounded selected GPU run
`artifacts/optical-cartesian-suffix-tight-selected-first.*`. Its independent
review passed all thirty frozen per-ray comparisons, verified identical
handoffs and routes, and retained all thirteen original-route terminals
exactly. The complete log audit also passed. The lead independently recomputed
and cross-reviewed those results. The first candidate's rejection remains
recorded; the original candidate and trace wrapper are intact.

Next: the lead and renderer are preparing the required branch and original
atmosphere correctness matrices with the fixed tighter control. Reuse existing
fixtures and the mechanical host; freeze original references before new
candidate outputs. The selected pass does not cover inversions, returning
rays, vacuum entry, invalid aperture, not-ready data or an executed fallback.
Verify synthetic packet consistency across both primary and field uniforms.
Only after those gates close may one matched complete-packet timing run.
No factor sweep, relaxed gate, full-frame timing or production integration
is yet approved.

The selected-ray baseline and prospective acceptance amendment are retained
under `artifacts/optical-cartesian-suffix-selected-baseline-first*` and
`artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json`.
The amendment was authored and independently reviewed before candidate GPU
output. It records baseline limitations explicitly and requires individual
ray comparisons; a better aggregate result cannot conceal a regression.
Original fixture limits still apply to their original fixtures. See the
math-owned design and policy reports for numerical definitions.

Current matrix ownership: rendering mathematics owns the branch adapter and
coherent empty-scene packets; the lead owns the atmospheric packet/launch
helper and prospective policy; the independent mathematics reviewer audits
the original baselines and candidate results. All cases reuse
`TraceFixture build(case, reference)` through
`scripts/optical_cartesian_suffix_branch_fixture.rs`. Root wired
`-CartesianSuffixBranch CASE` and `-BranchReference` into the existing host,
with selected-only and untimed restrictions. Independent source review and
all ten original offline/GPU/reference baseline checks have now passed. Their
summary is `artifacts/optical-cartesian-matrix-baseline-freeze-first-summary.json`.
The shared original source archive and mapping use the prefix
`artifacts/optical-cartesian-matrix-reference-first-source`. The candidate
inventory pins those freezes and the independently reviewed comparison code.
The candidate offline validation batch uses
`artifacts/optical-cartesian-matrix-CASE-candidate-validation-first`.
All ten validations and both independent assembled-source reviews passed.
The candidate GPU batch uses `artifacts/optical-cartesian-matrix-CASE-candidate-first`.
Its frozen per-case auditor passed readiness, fisheye, vacuum-outward,
vacuum-inward, atmosphere-minus20-vacuum, atmosphere-minus20-gradient and
atmosphere-100-vacuum. It rejected atmosphere-100-gradient and the batch stopped;
both atmosphere-5000 cases remain unrun. The failed report and raw outputs are
retained. No tolerance sweep, gate relaxation, full-frame timing or production
adoption follows. Root owns mechanical orchestration and engineering docs.

The rejected candidate sources are separately archived and hash-verified under
`artifacts/optical-cartesian-matrix-candidate-first-source*`; all eight candidate
executables were retained individually. The decisive independent stop report
is `artifacts/optical-cartesian-matrix-candidate-first-stop-review.json`, and
the lead has cross-reviewed it.

The new experiment uses `scripts/optical_original_clearance_fixture.rs` and
`scripts/optical_original_clearance_inputs.txt`, with root's
`-OriginalClearance` / `--original-clearance` host flag. Rendering mathematics
authors the fixture, the lead reviews its control flow and focused tests, and
the independent reviewer checks assembled sources and output gates. It reuses
the generic module/dispatch/state interface. Its existence establishes no
performance result and does not approve production integration.

The isolated adapter and mechanical host passed offline validation under
`artifacts/optical-original-clearance-validation-first`. The first source
checker failed because an expected string contained an extra closing
parenthesis; its source and failure log are retained. The independently
reviewed one-character checker correction passed the complete source reversal
and five deliberate-corruption checks. Shader bytes did not change. The
independent review also verified declarations, constants and all five uploaded
buffers. The bounded GPU run `artifacts/optical-original-clearance-selected-first`
then completed. The lead independently found exact original terminal/launch
parity, with both fast and fallback paths exercised. The independent provenance
and route audit passed and was cross-reviewed. This mode resets all required
per-frame buffers inside its dispatch sequence, including during timing.

Full offline validation and source review passed under
`artifacts/optical-original-clearance-full-validation-first`. After independent
approval, a matched original/candidate pair retained complete raw output and
timings together under `optical-original-clearance-full-reference-first` and
`optical-original-clearance-full-candidate-first`. Full terminal and launch
parity passed before timing interpretation, but the frozen performance gate
failed. The independent report and hash-linked closure are retained under the
candidate prefix and both were cross-reviewed. This split is rejected; it is
not integrated into the app. No repeated timing or scheduling sweep is planned.

The fresh original executable was copied and hash-verified after its successful
run; `-executable-retention.json` records that provenance. The candidate wrapper
retained and executed its own per-run binary. Earlier selected comparator,
proof and runtime-auditor versions are archived with their source hashes before
the fixed full-reference and guarded-timing support was added.

The lead authored `scripts/optical_handoff_feasibility.py` and its input
inventory to test continuation from the retained handoffs. The independent
reviewer approved the source and root's bounded CPU runner after the starting
layer checks were corrected; the review is retained as
`artifacts/optical-handoff-feasibility-source-review-first.json`. Root ran
`scripts/verify_optical_handoff.ps1 -Name optical-handoff-feasibility-first
-TimeoutSeconds 300`. All eleven cases report passing, with no skipped cases,
stable inputs and a successful lifecycle. Its two explicitly separate
initial-direction hypotheses are recorded in the report. Independent result
review passed and was cross-reviewed; see the run's `-independent-review.json`.
All workspace inputs are archived with verified hashes in `-source.zip` and
`-source-archive.json`; external runtime files remain fingerprinted separately.
This establishes no GPU implementation or performance
result; canonical versus uploaded-field behavior remains a separate question.
Do not repeat the rejected path-accumulation-only correction or tolerance-factor
sweep. The lead owns the next implementation-relevant precision design.

The fixed uploaded-field follow-up is exposed by
`scripts/verify_optical_handoff.ps1 -Name optical-handoff-packed-field-first
-PackedField -TimeoutSeconds 300`. It uses the existing entry and a separate
math-owned helper, with the expanded input inventory. Both source reviews
closed; the retained source report is
`artifacts/optical-handoff-packed-field-source-review-first.json`. The run
completed successfully with stable inputs, no timeout and all eleven cases
reporting pass. The independent raw-result audit passed and was cross-reviewed;
see `-independent-review.json`. The 27 workspace inputs are hash-verified and
preserved in `-source.zip` with `-source-archive.json`; external runtime files
remain fingerprinted only. The mathematics agents are scoping one native
f64 trial accuracy/cost control, with no controller or runtime integration yet.
Use an existing diagnostic host and preserve full failure evidence. A component
timing cannot be presented as a complete-ray lower bound or native FPS forecast.
No GPU kernel or app
change has been made. Root confirmed read-only local support
for the optional native shader-f64 feature in Naga/WGPU and the Vulkan adapter;
this is a capability observation, not a performance result.

The earlier immutable-face preparation remains separate from this matrix.
The patch certificate preparation entry is
`scripts/prepare_optical_patch_certificates.rs`; the mechanical wrapper is
`scripts/prepare_optical_patch_certificates.ps1`. It retains the exact input
inventory, hidden-process lifecycle, immutable face data, modified address
slots and three eye-metadata samples. Source review closed and CPU preparation
completed under `artifacts/optical-immutable-patch-second.*`. Its independent
all-face and three-eye packet audit passed and was cross-reviewed; the
independent report and its exporter-provenance companion are retained.
The first attempt failed at Rust macro parsing and
never executed; its failure note is retained. The wrapper now also saves
compiler input fingerprints and output before attempting execution.
This packet is not yet wired into the app or scene consumer.

Keep uncertain earlier or overlapping intersections explicit; do not relax
their acceptance predicates to make a test pass.

## Coordination and reproducibility

Mathematics agents: `/root/math_geodesy`, `/root/math_rendering`, and independent
reviewer `/root/math_geodesy/math_review`. Their existing chats retain the
detailed derivations and reviews.

Run GPU jobs serially, use bounded watchdogs, and keep the source/runtime
inventory frozen through each run. Do not overlap CPU cost timing with shader
compilation or GPU benchmarks. Every evidence run needs a fresh prefix.
Wrappers preserve inputs, executable hashes, logs, raw outputs and lifecycle
records. Failed attempts must remain available.

The workspace contains extensive preexisting uncommitted project work on
`codex/horizon-performance-stars`. Do not reset, clean, broadly revert, or
commit unrelated files. No new asset was added during these latest diagnostic
experiments. Distribution still requires the repository's license audits and
Windows packaging script.

See `docs/development/refraction-performance.md` for host commands, diagnostic
scope, focus policy and capture requirements.
