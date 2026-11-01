# Original primary shader: bounded loop-counter control

This diagnostic changes only the compiler policy for redundant loop counters.
It uses `ShaderRuntimeChecks::checked()` and sets only
`force_loop_bounding = false`. Buffer and array checks, integer-division checks,
WGSL validation, physical equations, numerical tolerances, authored loop limits
and all fixture data remain unchanged. It is not production authorization or a
claim of improved accuracy or native frame rate.

The admission gate is
[`optical_primary_termination.rs`](../../scripts/optical_primary_termination.rs).
It requires byte equality with the retained original full-frame shader and
2240-byte uniform, together with the exact 1920×1080 extent, 2,073,600 records and
64,800 groups of 32 invocations. The
[`proof manifest`](../../scripts/optical_primary_termination.json) records the
SHA256 digests and every loop in all 26 declared functions. Source changes,
different uniforms, selected-ray fixtures, canonical solvers and altered
dispatches are refused. A matching digest alone does not replace the runtime
byte-equality check.

The finite-termination argument is about integer control, not convergence of a
floating-point trajectory:

- The field iteration, binary searches, vacuum searches, height roots, face
  roots and outer ray attempts have fixed integer limits, respectively including
  5, 8, 40, 32, 20, 24 and 4096. Early returns, breaks and rejected attempts cannot
  increase these limits. Their counters cannot overflow at those limits.
- The profile count is the exact admitted integer 45. For the more general
  arithmetic premise `2 <= count <= 128`, the search preserves
  `0 <= lo <= hi <= count - 1`. Consequently
  `first = max(lo - 1, 0)` and `last = min(lo + 1, count - 2)` give at most three
  inclusive iterations, with `last <= 126`. There is no unsigned wrap in that
  loop. The broader premise explains the proof; this control still admits only
  the exact retained uniform.
- Each terrain traversal and its nested patch traversal share one visit counter.
  Every entered iteration increments it and returns above 8192 before any
  pointer-dependent continuation. Thus a call makes at most 8193 increments,
  including its terminating increment. Marker traversal similarly stops by
  increment 2049. Cyclic or nonmonotone links cannot defeat these guards; all
  memory-access checks remain enabled.
- Terrain leaf counts are masked to integers from 1 through 8. Marker corner and
  face loops have fixed limits of 8 and 14. The complete function graph is
  acyclic; validated WGSL has no recursion here.

NaNs, unresolved roots or a physically stationary attempt do not bypass these
integer guards. This establishes finite termination under the admitted source
and execution semantics. It does **not** establish acceptable latency, exclude a
driver/device failure, or prove a geometrically malformed packet correct. The
external process watchdog and the existing source/packet provenance checks
remain necessary.

The source tests enumerate all 16 `for` loops and five `loop` statements, check
the complete call graph and source hashes, enumerate the profile-search integer
states, and check the traversal guards. Separate Rust tests exercise successful
admission and rejection after source, uniform, extent and dispatch mutations.
These are finite control-flow tests, not optical-accuracy tests.

The single matched performance control uses the existing eight serial GPU
timestamp pairs. Iterations 0–2 are the predefined warmup; all five remaining
valid pairs are retained. Compilation, readback wall time and GPU-dispatch time
are reported separately. Both runs must use identical shader/input bytes,
adapter, dispatch and output layout; only the declared loop-counter policy
differs. The complete raw output, including diagnostics and header, must match
byte for byte. Every timestamp pair must be valid and strictly ordered.

The explicit decision rule is to continue integration work only when the
candidate median is at most 80% of the checked baseline median **and** the
slowest measured candidate dispatch is faster than the fastest measured baseline
dispatch. Otherwise this performance branch stops, without parameter sweeps.
The 20% threshold is a work-prioritization rule. Separation of these two sets of
five measurements is descriptive evidence, not a confidence interval or a
statistical-significance claim. Invalid evidence or any output mismatch rejects
the control regardless of speed.

Passing that rule would justify a separately reviewed broader admission policy
and native moving-view validation. It would not meet the user's smooth-flight
requirement by itself: this headless primary dispatch excludes stellar work,
shading, UI and presentation. Actual moving phases, full-resolution optics and
their existing readiness/provenance gates remain the acceptance test.

The matched control is now complete and **rejected for immaterial speed gain**.
The [independent decision record](../../artifacts/optical-primary-loop-control-finite-first-independent-review.json)
retains the complete byte comparison, source and upload hashes, raw timestamp
conversion and both predeclared decision predicates.
The retained [checked run](../../artifacts/optical-primary-loop-control-checked-first.json)
and [finite-loop run](../../artifacts/optical-primary-loop-control-finite-first.json)
have identical WGSL, all five uploaded buffers and all 165,888,016 output bytes,
including the header and diagnostic records. Input-stability, lifecycle and
timestamp checks pass. The five retained checked times are 323.39124,
324.34252, 324.80056, 323.68320 and 323.76448 ms; the corresponding candidate
sample set is 320.49076, 322.26888, 322.26460, 321.31584 and 321.16588 ms.
Their medians are 323.76448 and 321.31584 ms, a 0.756303% reduction. Although
the measured ranges are separated, the predeclared 20% effort gate fails.
There will be no sweep or production integration of this control. Production
retains compiler loop bounding and all other checked runtime policies.
