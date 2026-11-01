# Original-trace clearance control

This diagnostic retains the original midpoint integrator, field evaluation,
source-layer restrictions, local tolerance, attempt/path limits, h0 coverage
and terminal rules. It does not resume or modify a state produced by another
solver. Its purpose is to test whether separating the costly face-refinement
code from rays with no possible face contact reduces complete GPU work.

The fast module retains each original scene traversal, box rejection and
`face_event` expression through the `triangle_candidate` decision. The two
original empty-event returns remain: degenerate face area and failed
candidate rejection. Any other face sets a private fallback flag. That flag
must leave the traversal and be checked at the original trace's scene-event
call before its result is interpreted or a state/terminal is committed. The
fast module never treats this placeholder empty event as proof of clearance.
Original traversal-limit ambiguity remains subject to the original handling;
it is not silently converted into a miss.

For a ray that never sets the fallback flag, every visited face returns the
same original empty event, and every scene operation, h0 decision and accepted
integration step is therefore the original one. This is induction over the
unchanged trace control flow, not a new geometric approximation. A flagged
ray publishes no provisional terminal. A separate module reruns the original
trace from the exact stored launch, with its complete original attempt budget.
That result supplies the terminal. The fast prefix contributes only to work
counters, not the final physical path or direction.

All per-record phase, route, work counts and launch storage must be overwritten
on each sequence. The output header is cleared before work and published only
after complete-record validation; a missing fallback cannot expose a stale
terminal. The candidate retains ordinary checked shader creation. No new
interval allowance, tolerance, source-data eligibility or contact policy is
introduced. Readiness, masks and all original terminal statuses remain valid.

Source extraction must fail on unexpected call counts or changed matched
bodies. Independent review must inspect the assembled fast module for all
fallback propagation sites and the complete fallback trace for original
expressions. GPU compilation can change finite operation evaluation even when
the source arithmetic is unchanged: selected and full-frame terminal-word
parity are mandatory acceptance gates, rather than consequences asserted from
the source argument. Packet identity, every record and fallback counts must
be retained. Missing positive fast and fallback cases block coverage claims.

After selected correctness and full assembled-source checks pass, one matched
full-packet control may retain complete readbacks and raw timestamps together
to avoid a redundant large capture. Complete full-frame terminal/launch parity
must pass before any timestamps are interpreted. Measure the entire
fast/fallback/validation chain, including work on discarded prefixes and
buffer traffic. The existing effort rule requires at least a
20% median reduction and separated five retained sample ranges; it is not a
statistical significance or native-frame-rate claim. Stop on parity failure
or immaterial gain, with no scheduling sweep. The retained [air-stub diagnostic](../../artifacts/optical-trace-performance-1920x1080-g32-air-q16serial.json)
has a five-sample median of 200.06464 ms from positive, separated raw timestamp
pairs. That different workload is not a mathematical lower bound or a forecast
for this candidate. This remains a possible partial improvement, not evidence
that scene separation alone provides smooth moving full-resolution flight.
