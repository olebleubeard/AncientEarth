# Optical integration performance investigation

This is the numerical design and verification record for the current performance
investigation. The diagnostic implementations in
`scripts/optical_step_diagnostic.rs` do not, by themselves, change the production
solver or establish an application frame rate. GPU adoption requires separate
finite-precision, first-contact and native-flight checks. The atmospheric model
and its observational limitations remain those in [refraction.md](refraction.md).

## Physical equation and source layers

The independent variable is geometric path length `s`. Position `p`, unit
direction `u`, and optical path `I` obey

```
dp/ds = u
du/ds = (grad(n) − u dot(u, grad(n))) / n
dI/ds = n.
```

The weather-derived phase index `n` is still piecewise linear in geodetic height.
Within one source interval, `n(h) = n_anchor + slope (h − h_anchor)` and
`grad(n) = slope up(h)`. No weather sample, source knot, wavelength, terrain face,
marker dimension, or physical surface is smoothed to improve performance.

An explicit Runge–Kutta trial uses one fixed source interval for every stage.
Crossing a source knot is a separately resolved event; the subsequent trial uses
the adjacent interval. Sampling different gradients inside an unconstrained
high-order trial produced materially worse results in the initial experiments.
Similarly, merely retaining a large step proposal after a knot was insufficient
to preserve the old solver's measured accuracy.

## Embedded integration and continuous extension

The diagnostic uses the published Dormand–Prince 5(4) pair. Its rational tableau
and the Shampine continuous-extension coefficients are checked against
[Hairer and Wanner's primary DOPRI5 reference](https://www.unige.ch/~hairer/prog/nonstiff/dopri5.f).
The implementation is original; these are mathematical coefficients, not a
copied third-party solver implementation.

For the high-minus-low embedded discrepancy, the acceptance norm is the maximum
of position discrepancy, step length times direction discrepancy, and optical
path discrepancy. Large common position and index contributions are subtracted
algebraically before weighted sums: the tableau row sums and error-coefficient
sum identities make this equivalent in exact arithmetic and reduce floating
point cancellation. Accepted endpoint directions are normalized to their exact
unit-length invariant. The diagnostic retains the existing 1 mm local tolerance
while comparing 2 km, 20 km and 100 km step ceilings.

An embedded local estimate is not a bound on accumulated global error. A higher
step ceiling is admissible only after comparison with independent tighter
solutions, explicit layer events and physical first-contact checks. A finite
attempt/path budget still fails closed rather than fabricating a resolved ray.

The continuous extension evaluates a polynomial within an already computed
trial. In normalized path coordinate `t`, it has the form

```
y(t) = y0 + t [r2 + (1−t) [r3 + t [r4 + (1−t) r5]]].
```

This reuses the trial's derivative stages. It avoids recomputing the atmospheric
field repeatedly while locating a source-height event. Its exact polynomial
position derivative supplies Newton proposals; the independently interpolated
direction is not substituted for that derivative. Finite terrain/marker
contacts need their own conservative candidate and chronological event checks;
the clear-ray experiment does not replace those checks.

## Event safeguards

For a fixed physical source interval, the height second derivative is bounded
using the geodetic-height Hessian and the refractive curvature. A trial extension
of length at most `0.49 / maximum_index_gradient` keeps the exact unit-speed
branch index above 0.51 when it starts at `n >= 1`. Explicit tableau stages can
overshoot that exact branch because some coefficients are negative, so every
stage also checks a finite index of at least 0.5; the index is never clamped.

If the vertical derivative cannot turn before the next source knot, its
curvature bound supplies an upper crossing bracket directly. Otherwise the
conservative nearest-knot limiter remains. At a just-resolved knot, the first
departure step is also bounded against returning across that same knot. A
near-tangent case that cannot be resolved within the minimum step must remain
unresolved; same-sign endpoint heights alone do not certify absence of a layer
crossing.

Root refinement preserves a confirmed crossing bracket and uses safeguarded
Newton proposals. Iteration or arithmetic-stagnation exhaustion fails closed.
For the continuous extension, the physical ODE curvature bound is insufficient:
the polynomial itself needs a bound. With position coefficients `r2` through
`r5` and trial length `L`, conservative speed and acceleration bounds are

```
V = (|r2| + |r3| + |r4| + |r5|) / L
A = (2|r3| + 4|r4| + 2|r5|) / L².
```

Starting within the declared height domain, signed height can fall by at most
`V L`. Therefore a positive denominator
`rho_min + minimum_height − V L` bounds the height Hessian on the whole
polynomial, and `V² / denominator + A` bounds its height second derivative.
This is a numerical-trajectory root bound, not a proof of the true ray's global
error. A residual-to-path stopping test additionally requires a positive lower
bound on vertical speed and an allowance for height-evaluation error.

The proposed GPU event allocation is no more than one eighth of its existing
local position tolerance, with a 1 micrometre lower floor on path-bracket width.
The root allocation and its direction/optical-path consequences must be included
in the error accounting. It does not establish unchanged global accuracy merely
by being smaller than a local estimator. Quantized or grazing cases may still be
unresolved. GPU contact and independent-reference validation remain mandatory.

## Independent evidence and scope

The review uses an independent high-precision equatorial Bouguer-invariant
quadrature, with separate integrals on every source interval and explicit
tangent/returning branches. A second independent DOP853 calculation uses its own
WGS84 height/normal inversion and one-sided source events. Its tighter and
coarser solutions are compared with each other before they serve as references.
These are verification tools, not runtime dependencies.

The current evidence covers vacuum, below-reference starts, near-horizon layers,
a thin layer, a turning ray, and seven clear rays through the actual paired NOAA
profile used by the native terrain/marker fixture. Current diagnostic reports
are under `artifacts/optical-*-diagnostic.json` and
`artifacts/optical-*-reference.json`. They are retained engineering evidence and
are not runtime assets. Clear-ray accuracy and fewer field evaluations are
promising feasibility results, not first-contact certification or a claim that
the application now meets a particular frame-rate target.

## Auxiliary geodetic-state experiment

Repeated coordinate inversions at every Runge–Kutta stage can be avoided by
evolving the geodetic height `H` and unit outward ellipsoid normal `n` as
auxiliary state within a trial. This uses the exact WGS84 parallel-surface shape
operator, rather than a spherical approximation. Let `k` be Earth's polar unit
axis, `z = dot(n,k)`, `D = 1 − e² z²`, and define the standard principal radii

```
N = a / sqrt(D)
M = a (1−e²) / D^(3/2)
q = k − z n
t = u − n dot(u,n).
```

The auxiliary equations are

```
dH/ds = dot(u,n)
dn/ds = t/(N+H)
        + [a e² / (D^(3/2) (M+H) (N+H))] dot(u,q) q.
```

To derive the second equation, the east and north tangent directions have
normal derivatives `1/(N+H)` and `1/(M+H)`. The north projection is
`q qᵀ/(1−z²)`, while `N−M = a e²(1−z²)/D^(3/2)`. Cancelling that common factor
gives the expression above without a longitude or polar division. At a pole,
`q=0` and `M=N`, so its limit is isotropic and finite.

The exact equations preserve the unit-normal constraint. The current diagnostic
defines its off-unit extension by evaluating the physical field with
`n_hat = n_aux / |n_aux|`, without overwriting the raw Runge–Kutta stage state.
The exact unit-normal solution is unchanged; this is a specified smooth
extension, rather than selective mutation of intermediate stages. Every stage
rejects nonfinite or zero normal norms and nonfinite or nonpositive `D`, `M+H`
and `N+H`.

In the first, stage-only diagnostic, each trial begins with the existing coordinate inverse. At its endpoint, that
inverse independently checks the auxiliary height and normal against the
integrated position; the next trial starts from that independently recovered
geometry. Auxiliary embedded errors and endpoint discrepancies join the
acceptance norm. Normal comparisons require positive dot products and finite
nonzero norms. The quantity `sqrt(2) |a cross b| / (|a| |b|)` bounds the
normalized-vector chord when the angle is below 90 degrees; it is converted to
metres using a conservative principal-radius scale. The embedded comparison
uses the equivalent cross product of the high-minus-low error with the high
normal, avoiding subtraction of nearly identical unit components. This removes
irrelevant radial amplitude from a physical angular comparison without
enlarging the metre budget. An endpoint check alone cannot prove
absence of interior interpolation drift, so this experiment also requires the
independent invariant/ODE references and contact validation. The GPU's finite
normal precision must be assessed separately before this becomes a runtime
optimization.

## Direct continuous-output diagnostics

Repeatedly reintegrating a whole Runge–Kutta trial while locating a height or
triangle-plane root can cost much more than the accepted integration steps.
Two additional, unadopted diagnostics therefore check a continuous trajectory
before using it directly for event searches. They retain the same source
intervals and numerical budgets; neither changes the weather field.

The inexpensive diagnostic compares the quartic output above with the cubic
Hermite polynomial that has the same endpoint values and endpoint derivatives.
Their exact difference is

```
quartic(t) − cubic(t) = r5 t² (1−t)².
```

On `0 <= t <= 1`, its raw vector-norm supremum is `|r5|/16`. That is an exact
bound between these two polynomials, **not** a bound on their common error
against the physical ODE. Shared stage errors or a small cancelling `r5` can
make this estimator optimistic. The diagnostic adds this interior allowance
to the embedded endpoint estimate instead of replacing it or increasing the
local tolerance. Independent full-path and interior-event references remain
necessary before GPU adoption.

A more expensive audit comparator computes one full step and two consecutive
half steps. It restricts the full quartic to each half interval, converts its
difference from that half-step polynomial to degree-four Bernstein controls,
and uses their convex hull to bound the difference over the entire interval.
For power coefficients `a_k`, the controls are

```
b_i = sum(k=0..i) [choose(i,k) / choose(4,k)] a_k.
```

The second half starts from the first half's actually normalized accepted
direction. This avoids comparing a hypothetical unnormalized trajectory.
Its root bracket is confined to one smooth half polynomial, because the
normalization at their join can introduce a small derivative discontinuity.
The diagnostic uses the two-half trajectory and adds the whole-interval
comparison allowance to the retained embedded checks. This also remains a
numerical error estimator, not a proof against the exact ODE.

Both comparisons handle direction normalization explicitly. For a chosen unit
reference `q`, positive Bernstein controls of `dot(q,u(t))` give an interval
lower bound `m > 0` on the raw direction norm. A raw direction discrepancy
`delta` then bounds the normalized discrepancy by `2 delta/m`; multiplication
by the trial path length expresses it in the same metre-valued error norm.

Direct polynomial collision queries must use the **position polynomial's**
derivative and acceleration enclosure. The independently interpolated
direction and the physical ray's curvature bound cannot automatically be
substituted for them. A confirmed root, triangle-membership test, chronological
ordering, and finite ambiguity handling are still required. The independent
polynomial tests and reference-ray reports in `artifacts` establish only the
stated diagnostic cases; they do not establish a smooth native frame rate.

## Carried auxiliary continuous-output diagnostic

The next diagnostic carries the accepted auxiliary height and normal into the
next trial, instead of replacing them with a fresh coordinate inversion.
Its quartic continuous output includes both auxiliary quantities. Source-layer
contacts use the scalar height polynomial and its actual derivative; geometric
triangle contacts continue to use the position polynomial. Every accepted
prefix, including one shortened at a source contact, still checks its height
and normalized normal against an independent inverse of its position. The
embedded and interior estimates, this consistency discrepancy, and the contact
allocation must fit together inside the unchanged local budget. A failure
reduces the trial or reports unresolved; it does not snap the position or height
onto a source boundary.

The interior Hermite allowance also includes `|r5_H|/16` for scalar height and
the normalized-normal polynomial difference. A positive Bernstein projection
lower bound supplies the normal denominator, and the conservative principal
radius converts that angular discrepancy to metres. These extra comparisons
are numerical estimators and consistency checks, not a global error theorem.

Layer ownership persists explicitly across a resolved contact. Initial
selection compares the represented absolute source heights in order;
direction chooses a side only at exact equality. An unowned crossing even one
representable value below or above a knot must be resolved. Applying a broad
near-knot exclusion to this persistent state can otherwise keep the ray on the
wrong slope indefinitely. Likewise, subtracting the base height from an
already represented `base + knot` need not reproduce the original knot bits.
The diagnostic tests adjacent floating-point values, 5-nanometre offsets,
a non-roundtripping nonzero base, and a radial crossing whose optical integral
is independently two trapezoids across a real slope discontinuity.

The proposed GPU representation further stores height as a source-anchor
offset. For old and new anchors `a` and `b`, an accepted increment `delta_H`
updates that offset as `(a-b) + offset + delta_H`. A source-root residual is
`(a-target) + offset + delta_H(t)`. This reduces cancellation in a small contact
residual; its finite rebased representation still requires the consistency
check, and it is not a zero-height reset. Absolute
height is recovered where required by the WGS84 radii and independent
consistency check. The GPU port remains a diagnostic until its finite
precision, contact decisions, full-image differences and native performance
have been independently assessed.

## Six-state normal-coordinate diagnostic

The newer diagnostic removes Cartesian position from the integrated state.
It evolves height `H`, two coordinates of the surface normal, and the three
direction components. Position is reconstructed from those same coordinates;
there is no redundant height/position constraint to refresh after each step.
The weather column and canonical WGS84 geometry are unchanged.

In a fixed initial orthonormal basis `(e, north, n0)`, use the chart

```
w = n0 + a_chart e + b_chart north
sigma = sqrt(1 + a_chart² + b_chart²)
n = w / sigma.
```

The same parallel-surface normal derivative above gives
`a_chart' = (n'·e − a_chart n'·n0)/(n·n0)`, and similarly for `b_chart`.
The diagnostic rejects stages outside `a_chart²+b_chart² <= 1`; it does not
clamp the chart. Its physical position is
`P = N(n − e²(n·k)k) + H n`. Stable differences of this expression avoid
subtracting two Earth-radius Cartesian values. The GPU uses the canonical
WGS84 spheroid in the authored eye basis, rather than reproducing incidental
skew or anisotropy from a previously rounded implicit-matrix packet.
This is physical-model equivalence, not bitwise equivalence to that packet.

The dense output is polynomial in `(H,a_chart,b_chart,u)`, but its reconstructed
position is not a Cartesian polynomial. Its actual derivative is

```
P' = H' n + (N+H)n' − (N e²/D)(k−(n·k)n)(n'·k).
```

Scene contact tests must use this same curve and its physical derivative and
acceleration bound. They cannot substitute the separately interpolated
direction for `P'`. The coordinate error metric converts chart discrepancies
to metres with an upper principal-radius bound; raw-direction normalization
uses a positive Bernstein projection bound. Embedded and quartic-versus-cubic
differences remain local estimators, not accumulated-error guarantees.

Source-knot ownership is explicit. A resolved contact anchors the next branch
at that source knot with zero height offset and the appropriate one-sided
slope. The exact-height coordinate correction is charged to the existing
local budget. For root-time uncertainty, the contact speed bound includes
physical position, geometric/optical path, and trial-length-scaled normalized
direction, rather than position alone. A strict crossing bracket and positive
derivative lower bound constrain the root; an owned tangent departure instead
requires a signed Bernstein certificate after factoring out its initial zero.

The first seven complete GPU trials stopped at the minimum step near source
knots. Outward bounds on subtracting absolute source heights left a fixed
height-residual uncertainty even as the trial shrank. The corrected diagnostic
uses host-prepared split adjacent-node gaps with an explicit residual allowance,
exact nearby-float subtraction where Sterbenz's lemma applies, and a root
parameter represented directly in the trial's unit interval. The root retains
its interval and charge; no tolerance is enlarged. All seven repeated clear
rays then resolved. An independent comparison using the actual uploaded launch
directions found maximum endpoint, direction-vector and path differences of
0.352909 m, 4.73827e-7 and 0.103720 m, respectively, against the tighter
canonical DOP853 reference. The two reference settings differed by at most
5.64 nanometres in endpoint position. Exact-rational checks also enclosed all
47 encoded source gaps. These are finite fixture observations that include
source, coordinate and axis packing effects; they are not a global error
bound. The failed reports remain evidence. This diagnostic is not a production
solver until terrain/marker contacts, complete-image differences and native
performance have passed independent review.

The isolated interval implementation follows the applicable
[WGSL arithmetic and division requirements](https://www.w3.org/TR/WGSL/#floating-point-accuracy).
It widens tiny operands/results to normal-range bounds, checks overflow before
issuing arithmetic, and retains bitwise outward rounding between operations.
Division receives a larger representable-step allowance across binade
boundaries. These primitive checks do not certify arbitrary compiler behavior,
global ODE error or all physical contacts.

## Monotone-height parameter diagnostic

The next structural experiment changes the independent variable, not the
refractive field. On a segment with `v = u·n` bounded away from zero, height
can replace geometric path as the integration parameter:

```
d(a_chart,b_chart,u)/dH = d(a_chart,b_chart,u)/ds / v
ds/dH = 1/v
d(optical excess)/dH = (n_index−1)/v.
```

The state contains the two chart coordinates, three direction components,
geometric path, and the diagnostic optical excess. Descending segments use a
negative height increment, so path still increases in the normalized trial
parameter. Source heights are prescribed step boundaries. Stages stay on the
same one-sided profile branch; they do not integrate through a slope jump and
then repair its result. Split source-height arithmetic and any endpoint
representation correction still require their existing finite error charge.

Stage signs alone are insufficient. For the represented quartic, let `Q` be
the sum of the norms of its four chart-increment coefficients and `U` the sum
for its direction coefficients. The chart-normal map has differential norm at
most one. Thus throughout the trial,
`|v−v0| <= U + |u0| Q`. The diagnostic requires a positive signed lower bound
after its arithmetic allowance; `0.01` is an explicit conditioning threshold
for choosing this parameter, not a physical boundary or a visibility test.
It separately requires the Bernstein coefficients of the dense path
derivative to be positive. Near a tangent, turn or failed certificate, the
same refractive arclength equations must continue from the accepted state.
No geometric ray is substituted.

The error metric remains in metres. Chart discrepancies use the upper
principal-radius factor, normalized-direction discrepancies use a positive
raw-direction norm bound, and their length factor is at least both
`|delta_H|/v_lower` and the sum of the dense path-coefficient magnitudes.
Path and optical-excess errors contribute `|error_s|+|error_excess|`, which
also encloses their sum in full optical path. Embedded and interior estimates
remain local numerical estimates rather than a global accuracy theorem.

Terrain and marker handling must consume this very same dense curve.
`P_H` is obtained by differentiating the normal-coordinate position with
`H_H=1`; it is not replaced by an independently interpolated `u/v`.
The existing normal-map acceleration bound applies with `H_HH=0`, so a
height interval of width `d` has a chord tube bounded by `max|P_HH| d²/8`,
plus the unchanged numerical and finite-representation allowances. Candidate
ordering follows the increasing trial parameter and its certified increasing
path. Plane contacts use `normal_face·P_H`; the geometric and optical ranges
come from the same dense path coordinates. Reference `H=0` becomes another
prescribed height boundary, whose actual canonical terrain coverage is still
tested before returning a reference-surface hit. This is the integration
contract for a complete scene candidate; the unused CPU clear-ray experiment
does not itself establish GPU contact correctness or smooth performance.

## Sufficient scene-height ceiling

An independent preparation diagnostic supplies a conservative height ceiling
for all canonical terrain faces, survey prisms, current eye-facing sign plates,
and the `H=0` fallback. Signed distance to a convex ellipsoid is the supremum
of its unit supporting-plane offsets and is therefore convex. Within the
regular normal-coordinate domain it equals geodetic height. Consequently,
the height anywhere on a planar triangle cannot exceed the largest signed
height of its vertices. This statement does not treat the terrain as a
constant-height surface.

The preparation avoids accepting an unchecked inverse-coordinate result as
a bound. For a canonical point `P`, write `S=PᵀQP`, with the actual WGS84
ellipsoid matrix. If `S>1`, the radial point `P/sqrt(S)` lies on the ellipsoid,
so the nearest-surface distance is no greater than
`|P|(S−1)/(sqrt(S)(sqrt(S)+1))`. Inside the ellipsoid, zero is an upper bound.
Outward binary64 operations evaluate this sufficient radial bound. The
maximum includes every retained terrain vertex and all eight prism plus four
sign corners. Scalar float32 packing is upward; an exact zero stays zero,
while a positive tiny bound is at least the smallest normal float32 value so
flush-to-zero cannot invalidate it.

The actual cached 100-mile scene has 311,923 terrain vertices and 972 current
prism/sign corners. Its unused canonical ceiling is 166.4064954508793 metres,
packed upward to 166.40650939941406 metres. This number alone does not certify
the existing GPU representation. Any proven vertex-position error adds to
the ceiling by the signed-distance function's 1-Lipschitz property. The
whole-curve lower-height bound must separately account for coordinate/map,
integration and contact uncertainty. Only a strict inequality above that
final ceiling over the entire trial permits skipping its scene query. A
failed or unavailable certificate leaves the full query in charge. This is
an interval-local sufficient test, not an assertion that a ray is sky or can
never return to the scene. The diagnostic has no runtime effect.

## Certified Cartesian pieces for scene queries

The unused Cartesian producer keeps the accepted normal-coordinate curve,
its source ownership, and its existing numerical allowance. It converts only
the position map used by scene queries. On a dyadic interval `[a,b]` of width
`L`, let `P` be that canonical map and let `C` be its endpoint Hermite cubic.
For each Cartesian component, an outward bound `M4` on `|P''''|` gives
`|P-C| <= M4 L^4/384`. Endpoint position and derivative enclosures are retained
separately; they are not replaced by point samples. The corresponding
Bernstein controls are `P(a)`, `P(a)+L P'(a)/3`,
`P(b)-L P'(b)/3`, and `P(b)`.

The derivative enclosure does not differentiate the unknown intermediate
point in a value-remainder formula. Subtract the third-order Taylor
polynomial at the nearer endpoint. Its derivative remainder is bounded by
`M4 L^3/48`; the contributions from the opposite endpoint's value and
derivative are bounded by `M4 L^3/16` and `M4 L^3/18`. Reflection covers the
other half of the interval. Thus the separately justified sufficient bound
is `|P'-C'| <= 5 M4 L^3/36`. The stored quadratic derivative controls use the
original curve parameter, not the piece-local parameter.

Fourth derivatives are enclosed by factorial-scaled interval jets. For
`B=A^(-1/2)`, the identity `2 A B' + A' B = 0` yields
`B_n = -sum_{k=1..n}((2n-k) A_k B_(n-k))/(2n A_0)` for positive `A_0`.
The alternative scalar majorant uses the same recurrence with outward
nonnegative derivative bounds, the exact unit-normal identity, and an
explicit admitted chart domain. Failure of that domain or of any finite
bound refuses the certificate. The majorant is cheaper to construct but
can require more pieces; neither method changes the atmospheric field.

Each piece stores a split Cartesian origin and interval controls relative
to that origin. A face query adds the high then low origin parts to its
eye-minus-vertex enclosure before adding the local polynomial. This avoids
forming a distant absolute point and then subtracting a nearby face vertex.
Outward packing and evaluation errors remain in the result. Exact dyadic
endpoints permit the Hermite fitting remainder to vanish there, while their
canonical and packing uncertainty remains. No extrapolation outside the
stored piece is allowed.

The retained static packet preserves all 495 accepted height intervals. The
full-jet construction uses 546 pieces; the scalar-majorant construction uses
899. Every packed position radius is at most one quarter of the local
tolerance remaining after the original numerical and source-contact charge.
This is a subdivision/admission rule, not an additional error budget. The
consumer must still include evaluation, face/frame, root and returned-state
uncertainty in the same complete tolerance. Path, direction and height
remain the original dense coordinates. Every preceding piece must resolve
before a later contact can become the first event. The packet audits and
analytic bounds do not establish GPU contact correctness, complete-ray
accuracy or production performance.
