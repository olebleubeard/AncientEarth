# Weather-informed atmospheric optics

Atmospheric refraction is an optional optical mode using real NOAA forecast inputs or an explicitly labeled custom column. The same curved-ray model intersects the analytic WGS84 fallback, active canonical terrain triangles and survey marker prisms. Turning it off restores geometric viewing. Terrain, feet placement, collision, marker dimensions and source-backed categories retain their physical coordinates; only the viewing path bends. Eligibility and scene readiness are shared by rendering and measurements, and numerical failures remain unresolved. This is a weather-informed simulation, **not an exact reconstruction of today's atmosphere or a guaranteed prediction of a real mirage**.

The implementation is in [`atmosphere.rs`](../../crates/earth-math/src/atmosphere.rs), [`weather.rs`](../../crates/earth-math/src/atmosphere/weather.rs) and [`refractivity.rs`](../../crates/earth-math/src/atmosphere/refractivity.rs). GPU presentation and guide searches have additional finite limits described in [rendering.md](rendering.md) and [horizon-guides.md](horizon-guides.md).

In the view, bending can change the apparent horizon and the position or shape of the Sun and Moon. Different pixels follow different curved paths, so suitable profiles can produce compression, stretching or some mirage patterns. The range display reports the geometric length along the computed curved path; that is distinct from both the straight endpoint distance and the phase optical path defined below. Refraction does not change the Earth's physical size or move the observer. Whether a particular mirage appears depends on the supplied profile and the finite search/display limits, rather than on a scripted distortion effect.

## What the downloaded weather means

Global input is NOAA/NCEP GFS 0.25-degree GRIB2. Its actual surface `PRES`, surface geopotential `HGT`, 2m `TMP`/`SPFH`, **80m `TMP`/`SPFH`/`PRES`**, and complete pressure-level temperature/specific-humidity/geopotential triplets are used. The 80m values are genuine supplied fields, not an interpolated copy of 2m conditions; their height is geometric AGL, directly 80m above the provider ground. Pressure reduced to mean sea level is not substituted for actual pressure. [NOAA GFS inventory](https://www.nco.ncep.noaa.gov/pmb/products/gfs/gfs.t00z.pgrb2.0p25.f003.shtml).

Within the supported HRRR CONUS grid, the application also requests NOAA's finer **3km surface/2m conditions**: actual surface pressure/geopotential height and 2m temperature/specific humidity. The bounded public `wrfsfc` subset is used; it is not represented as a dense HRRR vertical sounding. GFS still supplies the vertical column above that lower reference. We do not derive specific humidity from the sparse dewpoint fields or silently request a pressure product through a filter that does not support it. Outside this region, or when regional data/composite checks fail, the validated GFS column remains an explicitly labeled fallback. [NOAA HRRR description](https://rapidrefresh.noaa.gov/hrrr/), [surface-product inventory](https://www.nco.ncep.noaa.gov/pmb/products/hrrr/hrrr.t00z.wrfsfcf00.grib2.shtml).

GFS chooses the nearest quarter-degree node. HRRR chooses the nearest post in its native Lambert conformal projected plane: the declared 6,371,229m sphere, 38.5° tangent standard parallel, −97.5° central meridian, and 3000m lattice. The strict decoder checks the GRIB projection/scan flags and subset registration rather than treating this as a longitude/latitude raster. Header coordinates are quantized to microdegrees; accepted lattice alignment differs by at most 0.25m. These are source registration choices, not a certified meteorological datum transformation. One selected column is extended horizontally along the entire optical path. It is not a measured three-dimensional air field or a model of centimetre-thick layers over a particular road or beach.

Live weather follows current computer UTC, independently of the astronomical calendar. Both products use the same nearest forecast valid hour. GFS requests a six-hour cycle at least six hours behind wall time, with older candidates; HRRR requests an hourly cycle at least two hours behind wall time. Availability is not guaranteed. Source model, cycle, valid time, selected post, cache status and fallback reasons remain visible. A live forecast profile is eligible only at its selected grid identity and within three hours of valid time. Moving to a new post or expiration removes stale input until replacement. Provider failure may retain an already valid snapshot with its existing timestamp; it never fabricates weather. `--weather-reference` and `--weather-regional-reference` are explicitly frozen verification snapshots, never live-data fallbacks.

Decoder checks cover source/time identity, required fields, duplicates, finite values, registration, pressure/height ordering and vertical coverage. The weather is forecast output, not a station observation at the eye. Surface skin temperature, although present in some responses, is not substituted for 2m air temperature.

## Custom columns

The custom editor starts from the retained temperature, pressure and specific-humidity samples of the current column, including the registered HRRR/GFS composite when present. Source heights remain fixed and appear in metres or feet. Temperature is editable in °C or °F, pressure in hPa, and specific humidity in g/kg of moist air; humidity is **not relative humidity**. Ground and 2m rows can be edited independently. Applying does not impose a new hydrostatic balance or copy edited ground conditions into other levels.

The same Ciddor calculation, piecewise-linear refractivity, lower continuation and 120km vacuum endpoint construct the edited profile. Values must be finite, within the existing numerical domains (100–400K, 0–1200hPa and 0–100g/kg), and retain strictly decreasing pressure and the existing profile/gradient limits. These checks establish numerical admissibility, not a physically observed or thermodynamically balanced atmosphere. Untouched fields retain their exact SI values through display-unit changes; applying an unchanged column preserves its refractivity layers and any existing scene anchor. The active source is labeled custom, with the original provider metadata retained separately as seed provenance rather than attributed to the edited air.

The six groups in [`custom_weather.rs`](../../crates/earth-math/tests/custom_weather.rs) cover unchanged GFS and anchored composite preservation, unit conversions and pending edits, independent lower-level edits, input rejection, inclusive temperature endpoints and custom-source provenance. They do not change or validate a new ray solver.

## One consistent vertical column and scene anchor

Geopotential height `H` is not WGS84 ellipsoidal height. The conventional [U.S. Standard Atmosphere 1976](https://www.ngdc.noaa.gov/stp/space-weather/online-publications/miscellaneous/us-standard-atmosphere-1976/us-standard-atmosphere_st76-1562_noaa.pdf) conversion is:

```text
R₀ = 6,356,766 m
z(H) = R₀ H / (R₀ − H)
GFS-relative height = z(H_level) − z(H_GFS_ground)
HRRR registration shift = z(H_GFS_ground) − z(H_HRRR_ground)
registered GFS height = GFS-relative height + HRRR registration shift
```

The composite retains HRRR ground/2m values and registered actual GFS 80m/pressure-level knots above that reference. The synthetic GFS ground/2m anchors are excluded: registering two competing 2m estimates only centimetres apart could invent a sharp layer. The first retained upper knot must have lower pressure, and all heights/pressures must be ordered; otherwise the regional composite is rejected in favor of validated GFS. Both columns must have exactly the same valid time. A conservative application policy additionally requires an absolute provider-ground offset no larger than **100 m**, a first retained GFS knot no higher than **200 m above HRRR ground**, and a cross-model join span of at least **20 m**. If the nearest actual upper knot violates these limits, the composite is rejected rather than silently discarding it. These are scope limits, not physical thresholds proving that a join is meteorologically correct. A continuous piecewise-linear refractivity join connects the retained knots. This preserves the two models' differing ground heights in a common conventional reference; it does not erase that difference by independently aligning both grounds. The modeled cross-model join can affect gradients and is not evidence of a measured inversion. Metadata retain both source URLs, both ground references and cycles, the registration offset, first joined height and join span. Rejected joins retain the validated GFS column with an explicit reason.

The selected lower column is then translated **once** onto the active canonical surface at the requested observer's horizontal position when a new source profile or scene is bound. This binding coordinate and its WGS84 surface height are recorded separately from the forecast grid post. Binding at the requested position matters when a small mountain import covers the observer while the nearest forecast post lies outside the crop. Let the fixed ground height be `h_base`. The optical field uses `h_relative = h_geodetic − h_base`. Smooth mode gives a zero base; imported terrain may give a positive or negative base. The immutable source/scene identity owns this registration; subsequent observer motion within that column keeps the original anchor. It is not recalculated at every ray point or changed to follow hills, so no unmodeled horizontal gradient is introduced. This is an explicitly transplanted forecast column, **not** an exact geoid/gravity conversion or reconstruction of the atmosphere at the downloaded DSM's resolution.

For downloaded columns, at ground and 2m, temperature and specific humidity use the supplied 2m values. Pressure at 2m is an isothermal hydrostatic estimate from actual surface pressure:

```text
Tᵥ = T₂ₘ (1 + 0.6078 q₂ₘ)
p₂ₘ = p_surface exp[−9.80665 × 2 / (287.05 Tᵥ)]
```

Here `q` is water-vapor mass fraction, not relative humidity. This lower-layer assumption cannot reproduce an unobserved thin thermal inversion. Below the fixed base, the declared continuation holds the lowest refractive index constant with zero gradient, only within the supported WGS84 domain down to −10km. It permits negative terrain and lower neighboring ground without an unbounded gradient extrapolation. It is an assumption, not additional weather data. The actual surface still stops the ray.

## Refractive index and field

The moist-air **phase** index follows the Ciddor calculation documented by [NIST, Appendix A21–A41](https://emtoolbox.nist.gov/Wavelength/Documentation.asp). The rendered model uses a single vacuum wavelength of **550nm** and a fixed dry-air CO₂ assumption of **450µmol/mol**. Specific humidity is converted directly to vapor mole fraction using the dry-air and water molar masses; an RH saturation/enhancement correction is not applied again. This avoids ambiguity about whether subfreezing relative humidity refers to liquid water or ice.

```text
x_v = (q/M_water) / [(q/M_water) + (1−q)/M_dry]
n−1 = (ρ_dry/ρ_dry,reference)(n_dry,reference−1)
    + (ρ_vapor/ρ_vapor,reference)(n_vapor,reference−1)
```

The source implementation includes the published wavelength dispersion and moist-air compressibility terms. Its numeric acceptance range, 100–400K and 0–120000Pa, is deliberately broader than the cited metrology validation range. Upper-atmospheric samples are therefore **extrapolation**, explicitly flagged in metadata. Published NIST uncertainty figures are not an uncertainty bound for this forecast column or its rendered horizon.

At each mapped height we calculate `n−1`, then linearly interpolate that quantity between knots. The field depends on WGS84 geodetic height alone: `n(x)=n(h(x))`. Above the highest retained pressure level, a declared linear continuation reaches `n=1` at 120km above model ground, where it joins vacuum continuously. This is a computational upper boundary, not a measured thermosphere model. The lower and upper assumptions, coarse source levels and ignored horizontal variability prevent claims of exact local mirages. WMO identifies mirages with curved rays, inversions and potentially multiple images; resolving those real conditions requires more detailed atmospheric evidence. [WMO International Cloud Atlas: Mirage](https://cloudatlas.wmo.int/en/mirage.html).

## Curved-ray integration and distances

For an isotropic stationary medium, unit propagation direction `u`, geometric arc length `s`, and optical path `ℓ`, the geometric-optics equations are:

```text
dx/ds = u
du/ds = [∇n − u(u·∇n)] / n
dℓ/ds = n
∇n = (dn/dh) U_geodetic
```

The last identity uses the outward unit normal of the actual constant-geodetic-height surfaces. It does not replace them with spheres or ellipsoids having axes `(a+h,b+h)`. The equations follow expansion of `d(nu)/ds=∇n`; see [Bourgoin, Zannoni and Tortora, equations 1–5](https://arxiv.org/pdf/1901.08461). Reciprocity permits tracing from the eye toward a surface or the sky. The solver retains altitude turning points; it does not assume that every ray ascends or descends monotonically.

The f64 measurement solver uses explicit midpoint integration with step doubling. Position, direction-scaled-by-step and optical-integral discrepancies all participate in the local error estimate. Its local target is 0.001m, maximum step 2000m, maximum 4096 attempts, and an atmospheric path budget of 2000km checked between steps. A terminating step may extend past that path budget by up to one step. These are finite numerical controls, **not a global trajectory error or physical accuracy guarantee**. Direction is normalized during integration. Conservative height-excursion bounds also limit steps at each refractivity knot, so a thin layer cannot be silently skipped just because all midpoint probes missed it. The CPU knot band is 0.00001m; the GPU uses a larger scale-dependent representability band and rejects unresolved layer spacing. A curvature/height bound reduces steps near the ellipsoid; ground and atmospheric-exit events are refined by reintegration and bisection. A sphere bounds entry searches from space, but the accepted entry is on the actual constant-geodetic-height boundary.

Physical triangles and marker faces use a conservative candidate search for each accepted integration interval. With unit-speed curve curvature bounded by `κ = max|dn/dh|` (the accepted index is at least one), a segment of length `L` lies within `κL²/8` of its endpoint chord. Candidate BVHs are expanded by that bound plus the local integration estimate and floating-point allowance. A segment-to-triangle distance lower bound narrows CPU candidates; ill-conditioned segment/edge pairs are retained rather than replaced by an unsafe endpoint distance. A chord miss alone is never accepted as proof that the curved ray missed an object.

Candidates are processed by chronological subdivision and reintegration. A signed face-plane crossing is bisected, then checked against the actual finite triangle. The CPU curve-deviation threshold is 0.0001m and terminal event interval 0.000001m, with a bounded 512-node event search. At a terminal shared-edge ambiguity, a verified event can carry ordering uncertainty within that interval; an unconfirmed curved contact returns unresolved. For a zero-gradient straight ray exactly coplanar with a triangle face, the implementation retains the canonical parallel-face convention: that face does not supply an isolated crossing; other crossed faces remain eligible. It does not promise to report every on-face tangency. These controls do not certify accumulated integration error or real-world accuracy. Reference `h=0` crossings are isolated on either side and masked only where the canonical normal-line terrain query has physical coverage. Thus a ray can pass through `h=0` to reach negative coastal terrain, without being teleported to an unrelated ground point. Terrain/marker hits are compared in the same path order, including vacuum segments. It is not a straight-ray intersection disguised by a screen warp. If the ray escapes, its outgoing direction continues through vacuum to the same apparent spherical Sun/Moon model used by astronomy. A ground or marker hit blocks that continuation. From space, the initial vacuum segment is tested first, so a nearer lunar sphere cannot be hidden by a later Earth hit. Numerical/path-budget exhaustion returns **unresolved**, not a fabricated ground hit or silent geometric substitute.

Three lengths have distinct meanings:

- **Curved geometric path:** `∫ds`, the distance along the computed viewing ray.
- **Straight chord:** `|target−eye|`, which ignores ray bending.
- **Phase optical path:** `∫n ds`; this is not automatically a pulsed-laser time-of-flight range, which needs group-index treatment.

The heading-specific refracted skyline searches actual curved-ray hit/miss outcomes, first at 5-degree spacing and then by 22 local bisections. It reports the nearest surface hit on the highest detected branch. A narrower separate duct/image branch can be missed; the final local angular bracket does not certify the global skyline or its range. L/V likewise use curved-ray endpoints under their documented finite screen search. V traces the selected guide pixel to the camera-facing plane at the actual curved skyline hit, rather than intersecting a straight substitute ray with that plane.

## Display and remaining limits

Enabled refraction with a valid profile and WGS84 eye height from −10 through 20km uses a dedicated curved-ray surface/sky pass. The 20km operational limit is inclusive and applies to the eyes, not the feet. Above it, rendering, range, skyline and L/V consistently use geometric mode with an explicit status; the downloaded profile is retained. This conservative display limit does not truncate the atmosphere at 20km or restrict the f64 reference solver. It cannot be reproduced correctly by uniformly shifting an existing raster image. The GPU samples the same physical profile, analytic fallback, canonical terrain faces and marker geometry, with independently tested observer-relative f32 geometry and a separate finite display error/budget. Unresolved pixels are identifiable. The shared [source-backed category lookup](water.md) retains regional boundary, native water-mask and global vector precedence; it changes surface color, not physical height, and is not photographic imagery. Decorative grass is explicitly hidden in this optical mode because its ordinary raster geometry is not included in this first curved-ray renderer.

The atmosphere is horizontally stratified, frozen for each selected forecast snapshot, monochromatic and isotropic. Wind advection, measured local optical turbulence and scintillation statistics, chromatic dispersion in the final image, extinction, clouds, aerosol scattering, refractive illumination/shadows and exact radiometry are not solved. Decorative stars separately use an assumed Hufnagel–Valley reference column and weak plane-wave Rytov strength, with a positive illustrative temporal process shared with the geometric view. These conditional reference statistics are not inferred from the GFS column, and intensity variation does not alter the viewing rays. See [the star presentation model and sources](rendering.md#celestial-geometry-light-and-stars). Artistic surface lighting and sky response do not become meteorological observations merely because viewing rays bend. Optical eligibility is not a prediction of whether haze, cloud, darkness or contrast lets a human see a target.

Independent checks use published NIST cases, exact radial optical integrals, straight-ray recovery, conserved equatorial Snell invariants with separate quadrature, and GDAL-decoded NOAA values. [`verify_weather.py`](../../scripts/verify_weather.py) reproduces the source fixture without using the production Rust decoder or weather formulas. [verification.md](verification.md) records actual completed checks and their limits.

## Closer agreement with an actual view

An exact reproduction of an arbitrary real view cannot be promised from this forecast input. The missing information is not repaired by a smaller integration step: the application does not know the continuously changing temperature, humidity and density along every light path. NIST distinguishes uncertainty in the refractive-index formula from errors in the atmospheric inputs, which can dominate a measurement. USNO likewise explains that variable atmospheric refraction prevents exact routine rise/set predictions. These support an uncertainty-aware simulation rather than a claim that a rendered horizon is today's measured horizon. [NIST input uncertainty](https://emtoolbox.nist.gov/Wavelength/Documentation.asp), [USNO rise/set accuracy](https://aa.usno.navy.mil/faq/RST_defs).

Further work would require site-matched air measurements at several heights and along sight lines, neighboring columns with their full spatial gradients, validated temporal interpolation, weather-dependent extinction/clouds, chromatic effects and refractive illumination. The implemented GFS80m and HRRR near-surface inputs add actual forecast information; they do not turn a single-column approximation into measured near-ground optics. Terrain and markers now share the curved path, but physical seeing conditions and illumination remain separate problems.

## Survey image branches

The center-dot pick uses the first resolved canonical terrain or prism event on the viewing ray. Its endpoint is physical ECEF and its range is curved geometric path length. Tape A-to-B values remain physical straight chords and separately labeled WGS84 geodesics, even when their endpoint images are refracted.

The optical tape image samples that unchanged ECEF chord, inverse-shoots each target through the field and checks earlier canonical terrain/prism occlusion. It starts with 16 intervals and refines visible transitions or a midpoint screen discrepancy above 0.2 physical pixel, with at most 1,025 samples. Its one-pixel annotation is not a second physical object. A target picked on a face can contact that face within the existing 0.005 m endpoint band. If clipping the forward query at that target leaves an edge contact unresolved, one unrestricted forward trace of the **same launch ray** may establish the first physical contact instead. This retry is accepted only for an actual terrain/prism contact within that same endpoint band; sky, a distant contact, another unresolved trace or cancellation cannot establish visibility. It changes neither the initial viewing ray, the optical equations nor any physical intersection tolerance; the unclipped query can use a different integration-step partition. The band is a numerical contact policy, not a source-accuracy or accumulated-ray-error guarantee. Finite sampling can miss narrow visibility changes and other mirage branches; unresolved annotation calculations remain unavailable.

Marker hidden height is a separate bounded inverse calculation. A straight-chord seed is iteratively adjusted to reach each centerline target through the field, using a target-plane residual, at most 16 iterations and a 0.002m numerical residual criterion. The current branch is sampled at 64 vertical intervals and detected visibility transitions are refined to a 0.001m height bracket. Each target is checked for an earlier canonical Earth/terrain hit; this excludes other markers by the declared Earth-hidden-height definition. Cancellation is checked throughout. A failed inverse or unresolved forward path makes the measurement unavailable rather than assigning a hidden/visible answer. Other mirage images and narrower height intervals can be missed; the result is explicitly a **sampled numerical image branch**, not a guarantee of all visible images. Local residuals/brackets are not source or total-trajectory uncertainty bounds.

Each extension should be checked against observations with known location, eye height, time and atmospheric data, while keeping source uncertainty distinct from numerical error. WMO describes mirages as potentially multiple, inverted, enlarged or reduced images caused by differing air layers; a convincing picture of one such pattern is not evidence that a particular real atmosphere has been reconstructed. [WMO Mirage](https://cloudatlas.wmo.int/en/mirage.html).

The optional source-verification environment uses the existing pinned
[`requirements-terrain.txt`](../../scripts/requirements-terrain.txt) tools. For
the repository's local Windows installation:

```powershell
$env:PYTHONPATH = 'D:\_Codex\AncientEarth\.cache\python'
python scripts/verify_weather.py --check
python scripts/verify_hrrr_column.py
cargo test -p earth-math --test refraction_review --locked --offline
```

Adjust the absolute Python path for another checkout, or install the pinned
requirements into a separate environment. GDAL is a verification tool only;
runtime ingestion uses the Rust GRIB decoder.
