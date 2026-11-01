# Projected Earth skyline guides

**Selected model:** smooth mode uses analytic WGS84 h=0; optional [downloaded elevations](topography.md) use the canonical source-post terrain and explicit h=0 fallback. Regional category triangles alone cannot raise, mask or change the horizon. Terrain-specific searches below are active when elevations are enabled.

**L** places a horizontal guide at the highest point found on the canonical Earth outline **inside the current viewport and lens aperture**. **V**, while L is enabled, measures the screen-down gap from that guide to the selected column's uppermost detected sky-to-Earth boundary. Both use the actual eye position, camera orientation including roll, lens and viewport. They do not change where the camera points. Left / Right arrows or the marker drag select V's column.

The active outline belongs to the selected physical Earth model. It excludes decorative grass, the Sun, Moon, stars and GUI panels, and does not depend on the artistic scene brightness. With atmospheric refraction disabled its rays are straight; with a validated active profile they follow the same f64 curved-ray equations and profile as the sight measurement. Atmospheric rendering is unavailable while downloaded elevations are enabled, so view, sight range and guides then all use geometric rays. The separate **Skyline ahead** readout uses a heading-plane calculation described in [visibility.md](visibility.md); it is not the same search.

Weather data can remain loaded while the optical display is unavailable. Outside the current 0–20 km geodetic eye-height range, the app explicitly selects geometric rays for the view, sight measurement and L/V together. A fallback notice remains visible in clean mode. The retained profile does not silently make the guides refracted over a geometric picture. This is an implementation limit, not a claim that atmospheric refraction ends at that height; see [rendering limits](rendering.md#weather-informed-curved-ray-presentation).

## What the lengths mean

The horizontal line spans the viewport, but its two reported lengths describe the **leftmost and rightmost selected visible outline points**, `Q_left` and `Q_right`. These points need not reach the viewport edges. Disconnected outline pieces are not asserted to form one continuous visible arc.

```text
L chord = |Q_right − Q_left|
L surface = shortest WGS84 ellipsoid geodesic between their latitudes/longitudes
```

The chord is a straight three-dimensional ECEF distance between the selected surface hits. The surface distance projects both endpoints to ellipsoid height zero and uses the robust Karney inverse geodesic algorithm, including near-antipodal configurations. It is a reference-Earth surface distance, not a terrain-following route or a length traced along the visible skyline. Terrain elevation, buildings and grass do not add path length. See [the WGS84 model](model.md) for the axes and flattening and [the geodesic implementation and sources](geodesics.md) for the inverse construction.

**V is labeled “drop below guide … at skyline depth / view-plane gap.”** A screen segment alone does not specify a physical length: its rays can pass through many depths, and the guide endpoint can point into empty sky. The app therefore states its depth reference explicitly.

Let `O` be the eyes, `F` the camera's unit forward axis, `Q` the measured skyline hit at the selected screen column, and `D` the unit ray through the guide at that same column. Construct the plane through Q perpendicular to F:

```text
depth = (Q − O) · F
P = O + D [depth / (D · F)]
(P − Q) · F = 0
V gap = |P − Q|
```

P is a **constructed reference point**, not a measured hit in the sky. P and Q project to the vertical screen segment's endpoints; their distance is measured in the specified camera-facing plane. This remains well-defined for the equidistant fisheye wherever both endpoint rays exist and have positive forward depth. It is not a geodetic height, gravitational vertical drop, surface distance, or an isolated measurement of Earth's curvature. Pitch and roll affect its reference plane and the guide location. No V value is displayed when the selected column lacks a usable sky-to-Earth boundary, the top endpoint lies outside the fisheye aperture, or the forward-depth construction is singular or behind the camera.

When refraction is active, Q is the curved ground-ray hit and P is the first valid forward intersection of the **curved top ray** with that same camera-facing plane. The straight formula for P above is then replaced by numerical integration and plane-event refinement; a vacuum continuation after atmospheric exit is permitted. The reported gap remains the Euclidean distance `|P−Q|` in that plane, not optical path length or gravitational height. If the top ray hits the ground before reaching the plane, fails its integration budget or has no usable crossing, V is unavailable. Multiple/turning optical paths are not resolved by pretending a straight sky ray meets the plane.

## Visible geometric contour

The current smooth WGS84 model needs no screen occupancy lattice. Let `A=diag(a,a,b)` contain the WGS84 axes, `O` be the eye, `o=A⁻¹O`, and `r=|o|>1`. In the scaled unit sphere, every tangent point satisfies both `|q|=1` and `o·q=1`. Its exact circle is

```text
C = o/r²
ρ = sqrt((r−1)(r+1))/r
q(t) = C + ρ(e cos t + f sin t),  0 ≤ t < 2π
P(t) = A q(t)
```

Here `e,f` are orthonormal and perpendicular to `o`. The factored expression for `ρ` avoids subtracting nearly equal squares near the surface. Camera components of `D(t)=P(t)−O` each have the form `a+b cos t+c sin t`; the constant part of D is evaluated as `−O(r−1)(r+1)/r²`, avoiding cancellation between Earth-radius positions. WGS84's defining axes and the authoritative source are documented in [model.md](model.md).

For rectilinear projection, forward depth and each viewport edge give linear sine/cosine equations. The projection extrema are also analytic. For a component ratio

```text
u(t) = (a+b cos t+c sin t)/(d+e cos t+f sin t)
numerator of u′ = (ce−bf) + (cd−af) cos t + (ae−bd) sin t
```

Solving these equations and retaining only forward, in-viewport points supplies the apex and clipped left/right endpoints, including tiny visible arcs that a screen lattice would miss. V solves the selected column's same tangent equation directly and selects a sky-to-Earth crossing using the direction derivative and outward surface normal. The tangent point itself supplies range and surface category; a rounded ray is not re-intersected to rediscover an ill-conditioned tangency.

For equidistant fisheye, the aperture condition is `Z≥0` and `sin²(α)Z²−cos²(α)(X²+Y²)≥0`, with half-field angle `α`. Four bounded `tan(t/2)` charts reduce each aperture boundary to a quartic; recursive derivative-root isolation locates its real roots. This explicitly clips narrow aperture arcs before projection refinement. On each visible arc, the projected derivative is sampled in 64 intervals and sign-changing extrema are refined; viewport edge roots are then refined between those extrema. This remaining bounded one-dimensional step is not a proof that every pathological fisheye extremum is isolated. It evaluates the analytic tangent contour, not thousands of Earth-ray integrations. Portrait clipping, camera roll and the circular aperture remain explicit.

Geometric points have no hit/miss search bracket, recorded as zero; this does **not** assert zero floating-point error. The analytic model, finite numerical root refinement, physical pixels and display mesh retain their distinct limits. The regional category boundaries cannot raise or hide this contour.

## Bounded refracted search and precision

With refraction active, L searches only the current physical viewport with an **8 by 4 interval** lattice. Valid rays with differing hit/miss states bracket a crossing; invalid aperture rays and unresolved integrations are excluded rather than treated as sky. Each crossing is bisected at most forty times to a local bracket of at most **0.025 physical pixel**. The three selected extrema receive at most four shrinking refinements, skipping endpoints already on the limiting viewport edge. Each L search permits at most **512 distinct optical trace attempts**, including failed attempts. Cached results retain the actual surface hit, avoiding duplicate endpoint integration. Budget exhaustion retains only already-resolved candidates or yields no usable result.

V searches 16 vertical intervals in its selected visible column, supplemented by the apex row and offsets of ±0.025 and ±0.1 pixel. It uses the first detected sky-to-Earth transition and the same bracket rule. If independently refined brackets overlap, V can use an **actual hit from the exact guide-row launch ray** while retaining a valid bracket no wider than 0.025 pixel. Coincident top and bottom launch rays then have the same measured endpoint and zero plane gap. Overlapping uncertainty intervals alone never imply zero gap. A genuinely higher column outline, failed plane crossing or unresolved optical query makes V unavailable.

This is a **sampled refracted silhouette estimate**, not a certified global maximum or complete multiple-image solver. Narrow, disconnected or ducted branches can be missed. The local bracket does not bound apex error, endpoint range error, forecast uncertainty or displayed mesh error. Tangencies are particularly ill-conditioned: a small angular change can produce a much larger change in range. No usable result is not proof that the view contains no horizon. The downloaded-terrain path uses a 64 by 36 lattice, canonical summit seeds and 128-interval V search. It too is finite and can miss narrow branches; it is not the smooth-Earth analytic contour.

## Scheduling and snapshot consistency

Results are cached by observer, lens, viewport, active surface and weather-profile generation. Smooth geometric contour calculations remain synchronous and small. Expensive optical/terrain L/V and their heading-plane skylines run in bounded background jobs. When a snapshot changes, the old guide is hidden and an updating status is shown; a completed result is accepted only if its snapshot still matches. Cancellation is checked between rays and within terrain vertex/BVH scans, with an individual bounded intersection or integration allowed to finish. Turning L off cancels its work. Moving V can reuse the current L profile while recomputing its column; changing units only reformats lengths.

This removes a seconds-long guide calculation from the interactive UI path without claiming that the curved-ray renderer itself is fast. Continuous motion can keep the optical guide unavailable while obsolete work is discarded; stopping allows the current calculation to complete. Clean view avoids the hidden heading-skyline calculation. Native checks and separately labeled guide microbenchmarks appear in [verification.md](verification.md).

## Physical pixels and display

For logical display dimensions `(w,h)` and framebuffer scales `(s_x,s_y)`, the physical viewport is `(W,H)=(w s_x,h s_y)`. Screen coordinates map to rays using

```text
NDC x = 2 x/W − 1
NDC y = 1 − 2 y/H
aspect = W/H
```

The smallest detected outline y is the guide's subpixel reference. L fills physical row `floor(y)`, clipped to the framebuffer; logical bounds divide by the corresponding scale. Non-antialiased filled rectangles give a one-physical-pixel line, including on HiDPI displays. V likewise uses a one-physical-pixel column. Labels use the unsnapped measurement coordinates; the displayed row is less than one physical pixel from that reference. Fisheye's black aperture exterior can contain the full-width guide but contributes no measured outline points.

GPU rendering has finite tessellation, f32 precision and antialiasing. Thus the guide's canonical outline can differ slightly from a visible display triangle or antialiased edge. Source-data coverage, terrain sampling and reference-ellipsoid limitations remain those documented in [terrain.md](terrain.md) and [rendering.md](rendering.md).

Implementation: [`guides.rs`](../../crates/earth-math/src/guides.rs). Independent verification is recorded in [verification.md](verification.md).
