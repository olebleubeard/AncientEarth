# Direct-output Cartesian precision workload

This is a distinct, fully observed workload. The earlier checksum workload
remains rejected: its reduced outputs do not identify or bound the difference
between detailed and timed calculations, and its timings remain uninterpreted.
This experiment does not rerun or accept that workload.

The eleven uploaded fields, raw saved handoffs, recorded source owners and new
standalone trial lengths are unchanged. The first accepted logged distance is
still not a reconstruction of an original attempted step. The lengths cover
about 124–8,400 m, not the later 100 km suffix steps. The original f32 and fully
promoted f64 Cartesian trial equations retain the previously reviewed
precision boundary and rational expressions. No root/controller, normalization
policy, field constant, error tolerance or physical model is changed.

Every evaluation in each timed kernel writes its own complete 188-scalar
result: the returned trial, five dense evaluations, all seven stage states and
field values, and embedded/interior estimator intermediates. Each scalar is
stored as its two f64 words; f32 values widen exactly. A separate detailed
kernel, sparse sampling or checksum comparison supplies no numerical evidence
for this workload.

The fixed workload remains eleven inputs, eight cyclic evaluations per
invocation and 256 repetitions. Each of 22,528 output records has eight u32
header words and 376 payload words, or 1,536 bytes. The output is 34,603,008
bytes (33 MiB) per dispatch. All three warmup and five measured dispatches for
each precision are retained: sixteen raw buffers total 553,648,128 bytes
(528 MiB). The host clears the complete output before each dispatch and copies
and retains it before the next dispatch can overwrite it. Both operations
occur outside the GPU timestamp interval. The timed dispatch includes every
numeric store. Every record requires its nonzero format magic, exact identity,
precision and all seven stage-written bits; missing or partial writes fail.

The auditor reads all 360,448 records. It verifies every header and complete
payload. Only exact equality of all payload bytes for the same input and
precision may reuse a comparison result; hashing alone is insufficient. Every
distinct payload is checked against the unchanged independent 100-digit
ordinary-tableau target. Both precisions must have finite valid complete
stages and positive norm lower estimate. All 75 promoted arithmetic groups
must meet the unchanged `B = 1/128000 m` limit with the previously declared
metre scalings. The f32 errors are reported, not required to meet B. The report
retains distinct payload counts, maxima and first failure locations. Any
numeric, identity, provenance or completeness failure blocks timing analysis.
If unusually many distinct payloads prevent the bounded audit from completing,
the result is inconclusive and timing remains uninterpreted. The host must
confirm space for the raw buffers, executable and reporting margin before
starting; prior evidence is not deleted to make room.

After all sixteen buffers pass those gates, the five non-warmup GPU durations
per precision may be compared. The fixed engineering effort rule remains a
promoted median no more than twice the original median and promoted maximum
strictly below twice the original minimum. Full stores and recording-related
register liveness can strongly affect this ratio. Clearing, readback, host
validation and file writing are excluded from the timestamps. This is a cost
for the observed workload, not intrinsic f64 throughput, an unobserved core
cost, a whole-controller bound or native FPS.

A failed fixed gate ends this workload without a size, length, tolerance or
precision sweep. Passing permits consideration of a separately reviewed
controller implementation only. It does not accept the earlier checksum
workload or establish complete-ray, source/contact, scene-clearance, fallback
or production correctness. All frozen full-ray/matrix gates and the complete
chain performance gate remain necessary before adoption.
