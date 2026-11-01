# Tape measurements and curvature markers

The survey tools use the selected canonical Earth surface and double-precision ECEF geometry. They add no terrain, surveyed objects or new photographic assets. Marker dimensions and coordinates define generated reference objects; they do not assert that those objects exist in the real world. The requested startup feet coordinate is **29.305549°N, 94.767015°W**, on smooth WGS84 `h=0`, with the configured eye offset above it. An explicit overview action can change that ground-level starting view.

## Row placement and units

Each prism is exactly 10 international feet wide, 10 feet deep and 100 feet tall in its local orthonormal frame. The international foot is exactly `0.3048m`, and the mile is `5280ft = 1609.344m`; [NIST](https://www.nist.gov/pml/us-surveyfoot/revised-unit-conversion-factors) supplies those definitions. Bottom-to-top paint bands are one foot high: the first band is red, the next green, and so on. The final 99–100ft band and top cap are green. Filtering distant paint does not change prism geometry or dimensions.

The default row uses a 100ft initial left offset, 0.5mi spacing and a 40mi reach. It contains **81 markers**, at stations 0, 0.5, 1.0, …, 40.0mi. Station zero is directly at the initial left-offset point, beside the observer rather than half a mile ahead. The distance settings are stored in metres, so changing GUI units preserves the physical layout. The application bounds work to 512 markers including station zero, spacing of at least one prism depth, reach at most 10,000km and initial offset at most 1,000km. These are explicit tool/resource constraints, not Earth-accuracy limits.

Let `(φ₀,λ₀)` be the observer's feet latitude/longitude and `α` the horizontal heading. Pitch and roll do not tilt the row. Placement first solves the WGS84 direct geodesic at azimuth `α−90°` for the left offset. From that start, each station solves the direct geodesic with the same initial azimuth `α` and distance `i×spacing`. This is one geodesic row after one initial offset, **not** a constant-offset parallel curve to a different geodesic. The robust Karney algorithm comes from the existing reviewed GeographicLib implementation; see [GeographicLib's geodesic definitions](https://geographiclib.sourceforge.io/html/python/geodesics.html) and [the project's geodesic model](geodesics.md).

Each station's base center is the active surface's ground query along geodetic up. The row's terminal forward azimuth orients its horizontal axes. With local east `E`, north `N`, up `U` and terminal azimuth `β`,

```text
forward = N cosβ + E sinβ
right   = E cosβ − N sinβ
up      = U,             right × forward = up
P(x,y,z) = base + right x + forward y + up z
x ∈ [−width/2,width/2], y ∈ [−depth/2,depth/2], z ∈ [0,height].
```

The base is a horizontal plane anchored at its center, not a separately draped footprint. On sloping terrain, parts can penetrate or stand clear of the ground, and actual Earth occlusion handles them. Generated positions are Earth-fixed once placed. Rebuilding a row on another surface is a new placement; changing a datum label does not move its objects.

## Number plates

Each marker carries a black, upright number plate with white digits. The displayed label is the zero-based marker identifier plus one, padded to four digits: station zero is `0001`, followed by `0002`. The same label is used in the survey controls. The plate is an annotation, not an extension of the measured 100-foot prism, and is excluded from prism picking, tape endpoints and hidden-height calculations.

The original digit shapes use five columns and seven rows. Four digits, three one-column spaces and a one-cell border make a 25-by-9 grid. Plate width equals the marker width; plate height is `width × 9/25`. Thus the default plate is 10 feet wide and 3.6 feet high. Its bottom remains at the prism's top height; for an external horizontal eye the plate moves to an eye-facing supporting plane of the box, so the roof cannot hide the digits. Black and white row-run rectangles partition the same flat plane, avoiding overlapping text polygons, textures or external font assets. Finite pixels, MSAA and projection still limit distant readability; the label is never enlarged according to distance.

Let `U`, `R` and `F` be the marker's fixed local up, right and forward, `C₀` the top-centered plate position and `E` the actual eye position. Its initial horizontal facing direction is

```text
D = E − C₀
H = R dot(D,R) + F dot(D,F)
normal = normalize(H)
plate_right = normalize(U × normal)
plate_up = U.
```

For half-width `a` and half-depth `b`, the box's support in horizontal direction `N` is `s=a|N·R|+b|N·F|`. If the external eye is beyond this plane, the center becomes `C=C₀+sN`. Every prism point has horizontal projection at most `s`, whereas every eye-to-plate segment before its endpoint has projection greater than `s`: the prism cannot block it. This is a horizontal placement correction, not an arbitrary increase in marker or plate height.

An eye just outside a corner can still lie behind that radial support plane. In that case, clamp its horizontal coordinates to the box to obtain the closest point `S=R clamp(D·R,−a,a)+F clamp(D·F,−b,b)`. Use `C=C₀+S` and `N=normalize(H−S)`. The rectangle-projection property makes this another supporting plane with the eye on its outward side. A one-micrometre guard rejects numerically indistinguishable support separation; an eye inside the horizontal footprint or within that near-contact guard retains the top-centered plate. No visibility guarantee is made at such finite contact, or against other objects.

The final normal is reconstructed as `plate_right × plate_up` to remove finite orthogonality roundoff. Orientation changes only about geodetic up: pitch, camera roll and the six fisheye camera directions do not tilt the plate. If the horizontal displacement is at most one micrometre, yaw is numerically undefined and the marker's forward axis is used. The top-centered overhead case remains edge-on; there is no artificial tilt toward the camera.

Static plate meshes are retained while the eye moves. Each frame converts the f64 center and axes to the same committed local display frame used by the marker, then updates only the entity transform. The opaque unlit black/white material preserves label contrast by design rather than modeling illuminated paint. Plates use ordinary scene depth in normal, telescope and fisheye views, remain present in F2 clean view, and cast no shadows. They obey the existing shared geometric-view override while survey objects are active.

## Picking and tape length

Prism picking transforms the ray into the same orthonormal box axes. For each axis it intersects the coordinate interval using the two slab parameters, then intersects all three parameter intervals. The input direction is normalized so the result is an eye-origin distance in metres. Parallel-outside rays miss; a ray starting inside a prism returns the positive exit boundary. The nearest box candidate is compared with the canonical Earth hit; an Earth hit more than a one-micrometre numerical tie guard nearer blocks it. Survey hovering therefore requires a prism boundary unblocked by canonical Earth or another prism. Decorative grass and celestial spheres are outside this survey-selection policy; the ordinary eye-range HUD separately compares its other measured targets.

Tape clicks select the nearest active Earth surface or visible marker boundary. Grass and celestial spheres are excluded from this tool. A and B retain their selected ECEF positions. The primary tape measurement is the Euclidean chord

```text
distance₃D = |B−A|.
```

The optional second value inverses A/B to geodetic latitude/longitude, projects them to WGS84 `h=0`, and solves the shortest ellipsoid geodesic. It ignores endpoint heights and is neither a terrain-following route nor a path over marker faces. A straight chord between distant surface points can pass inside Earth. In geometric mode the tape is deliberately an **always-visible screen annotation**, with its measurement printed beside it: the line can overlay Earth even where the actual chord would be underground. This is not evidence of an unobstructed sightline, and the annotation is not an additional pick target. In optical mode its image instead follows the bounded inverse branch described below and omits blocked portions.

The line projects the actual ECEF segment `P(t)=A+t(B−A)`, `0≤t≤1`. Rectilinear/telescope projection clips it against the forward view planes before division. Fisheye projection clips to the forward circular cone; the maximum of `z/|P(t)|` is found among the endpoints and its sole stationary parameter, then aperture crossings are bisected. Its curved screen image is subdivided using midpoint/quarter-point chord-distance probes with a 0.1-physical-pixel target, bounded to 24 subdivision levels and approximately 4,096 segments. These finite probes are a display criterion, not a global projection-error proof. A one-micrometre camera-front guard excludes singular points at the eye. Fully offscreen or unresolved annotations are omitted.

Projected segments are clipped to the viewport and expanded by half a physical pixel on each perpendicular side before conversion to logical UI coordinates. Thus nominal width remains one physical pixel with HiDPI and unequal framebuffer scales; diagonal raster coverage need not occupy exactly one pixel in each row or column. The label is anchored near the halfway point of the visible polyline and always reports the full endpoint measurement, even when some drawing is clipped. Label text and line thickness do not change A/B or the chord length.

## Height hidden by Earth

The hover readout measures the prism's **vertical centerline**, from its base center to its top center:

```text
T(z) = base + up z,     0 ≤ z ≤ height.
```

For every tested z, it casts the actual geometric segment from the current eye to T(z) through the selected Earth model. A nearer Earth/terrain hit means that centerline point is hidden. The endpoint comparison uses a guard `10⁻⁵m + 128ε max(segment_length,a)` so the supporting surface at the target itself is not mistaken for foreground obstruction. That guard and the numerical bracket are finite-arithmetic choices, not source accuracy claims.

Over smooth Earth, the prism centerline follows an outward geodetic normal. Convex ellipsoid geometry makes its shadowed portion a prefix from the base: a visible supporting point has its outward ray on the observer's side of the supporting plane; a hidden point can leave the convex shadow interval once. The eye must be outside the smooth Earth. Actual ray tests determine whether none, all or part is hidden; the transition uses at most 64 bisections to a height bracket at most `10⁻⁵m`. This does **not** substitute a spherical horizon-distance approximation. Tangency conditioning can still exceed the final local bracket's apparent precision.

Terrain can hide separate intervals, rather than only a lower prefix. That mode tests 1,024 equal centerline intervals and refines each detected transition with the same bracket rule. At the default height the sample spacing is `100/1024ft = 0.09765625ft` (about 0.02977m). It reports both the hidden prefix and the **sampled total hidden height**. Narrow hidden or visible intervals between samples can be missed; a finely bisected detected edge does not certify that all other intervals were found. Missing terrain coverage and the h=0 fallback are part of this modeled result. The method is explicitly labeled as sampled.

This readout does not claim that the entire rectangular cross-section is hidden up to the same height. A corner can be visible while its centerline is blocked. It excludes occlusion by other markers from the *Earth-hidden height*, although those markers still participate in picking. It is not atmospheric transparency, human recognition distance, tide prediction or an exact measurement of real terrain. The geometric method above omits refraction; the separately labeled optical method below follows the supplied model.

Placement and height queries support cancellation. Background results must match the same eye, marker and active surface snapshot before the UI accepts them. In eligible atmospheric mode, the same curved-ray scene intersects the canonical terrain and prism faces. Picks report physical endpoints and curved eye-to-target range; A-to-B tape lengths retain their stated chord/geodesic definitions. Marker hidden height uses a bounded inverse solve seeded by the straight direction, samples64height intervals on that image branch, and refines detected boundaries to0.001m. Other mirage branches and narrower intervals can be missed; unresolved calculations remain unavailable. These are numerical controls, not physical uncertainty bounds. See [the shared optical model](refraction.md#survey-image-branches).

## Validation and source scope

The implementation is [survey.rs](../../crates/earth-math/src/survey.rs). Independent review compares row stations with PROJ direct geodesics and uses a separate equatorial tangent construction for the smooth hidden-height threshold. A terrain fixture with a raised unsupported plateau checks separate hidden intervals and zero hidden prefix. Box slabs, nonunit rays, inside/outside starts, Earth blocking, tape chords, coordinate rejection and cancellation are reviewed separately.

The deterministic native demo uses the actual scene, not a second marker-coordinate table. In smooth mode it selects station 11.5mi, whose modeled hidden centerline height is approximately 14.9637m for the default standing eye. Its center ray is verified to pick a visible fragment of that prism. A separate close pose resolves the one-foot bands. Both supply a true 100ft vertical prism-edge tape pair. These are reproducible model checks, not observations. Final executed evidence is recorded in [verification.md](verification.md); GPU color/stripe/raster limits are documented in [rendering.md](rendering.md).
