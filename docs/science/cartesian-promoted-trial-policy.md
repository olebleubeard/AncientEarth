# Fixed Cartesian precision trial

This one diagnostic uses the eleven recorded raw handoffs and uploaded fields
from the independently passed packed-field CPU screen. The first logged
accepted distance becomes a **new standalone trial length** at each handoff.
It is not the original attempted step: the log omits rejected trials and the
full step before source clipping. These lengths range from about 124 to 8,400 m;
the screen does not sample the suffix's later 100 km steps.

The reference retains the original f32 geometry and seven-stage DP/Hermite
source. The candidate promotes the complete Cartesian arithmetic core to f64:
state/stage additions, uploaded-field inverse, refractive index and bend, RK sums,
normalization, dense coefficients and embedded/interior estimates. Original
rational expressions and exact widened uploaded constants are retained. No
initial direction normalization, ideal matrix, recomputed slope, new tolerance
factor, root/controller implementation or scene algorithm is introduced.

An independent 100-decimal-digit ordinary-tableau implementation evaluates the
same field with five fixed inverse updates. This is an arithmetic target, not a
symbolic identity with the GPU's finite fixed-point shortcut. The exact bound
is `B = (0.25 * 0.002) / 64 = 1/128000 m`, or 7.8125 micrometres. All candidate
groups must meet it: position and height directly; direction, normal,
direction coefficients and the derivative with respect to path distance
multiplied by the trial length; bend multiplied by length squared; stage
refractivity multiplied by length. Metre-valued embedded errors, Hermite
coefficients, `en`, `nr5`, interior and total estimators are checked directly;
the dimensionless norm lower estimate `m` is multiplied by length. Vector
groups use Euclidean difference. Dense fractions are exactly
`0, 1/4, 1/2, 3/4, 1`. There is no propagated optical-excess curve: only the
existing stage refractivities and optical estimator terms are checked.

Both modes must execute all seven finite valid stages with positive `m` before
cost interpretation. The f32 differences are reported; the new precision gate
applies to f64. No measured difference becomes an added allowance in a future
solver. This arithmetic screen proves neither a global ODE bound nor source
ownership, first-contact chronology, complete-ray accuracy or native speed.

The fixed cost workload uses eight cyclic evaluations per invocation and 256
repetitions of the eleven inputs, with three warmups and five retained timestamp
pairs. Uninstrumented trial functions supply all 32 returned scalars and the
same five dense evaluations; the separate detail functions retain stages and
error intermediates. Cost checksums consume both halves of every f64 output
(f32 results widen exactly). They must agree with detail-derived checksums
before timestamps are interpreted. This is checksum/source consistency, not a
collision-free proof of every value. Both workloads include the same output
encoding/checksum structure; this differs from controller consumption.

The prospective engineering effort rule requires candidate median time no more
than twice the reference median and candidate maximum strictly less than twice
the reference minimum. This **2× rule is not a whole-ray cost bound or an FPS
forecast**. Failure of inputs, arithmetic, checksum or cost stops this full-f64
scope; there is no size/length/precision-factor sweep. Passing only justifies
considering a separate controller port. That port must still preserve the
legacy prefix/fallback, source and h0 policies, outward whole-curve clearance,
all frozen ray/matrix gates (including turning), and the complete-chain 20%
performance gate before production consideration.
