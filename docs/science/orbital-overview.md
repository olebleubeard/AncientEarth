# Orbital overview and Earth-axis geometry

Orbit and Earth-axis geometry are annotations in the same continuously navigable first-person camera. There is no parked inspection mode, automatic fit, change of physical scale or replacement lens. The existing clock, apparent Sun/Moon rendering and active Earth surface continue at every height in the finite four-AU navigation domain. Explicit fit controls deliberately reposition that same observer; they preserve the selected lens and reject a required pose beyond the supported domain. See [continuous-flight.md](continuous-flight.md).

The scientific geometric states live in [`astro/orbit.rs`](../../crates/earth-math/src/astro/orbit.rs); projection, fit and display-frame helpers live in [`rendering/continuous.rs`](../../crates/earth-math/src/rendering/continuous.rs). The former separate heliocentric camera implementation remains as tested mathematical support where reused, not as a second runtime observer.

## Positions and the annual-scale trail

At each sample, the existing TT-to-TDB conversion supplies the time argument for the unmodified DE440s kernel. Let `P_i(t)` and `V_i(t)` be a body's barycentric ICRF position and velocity from that kernel. Body identifiers are Sun `10`, Earth center `399` and Moon center `301`. The overview evaluates:

```text
r_E(t) = P_399(t) − P_10(t)
v_E(t) = V_399(t) − V_10(t)
r_M(t) = P_301(t) − P_10(t)
```

These source vectors have their origin at the Sun and directions in fixed celestial axes. For display they are translated relative to the current Earth center and rotated into the current ECEF camera frame; the eye is not moved. Every trail sample subtracts the Sun at that sample's own epoch. Holding today's Sun position while sampling other dates would describe a different curve. These are simultaneous geometric coordinates, without light-time delay or observer aberration. The translated Sun-centered view is a diagram of relative states, not a relativistic transformation into a freely falling observer frame. [JPL's Horizons manual](https://ssd.jpl.nasa.gov/horizons/manual.html#defs) distinguishes geometric states from observed/apparent positions and defines its ICRF directions.

The Earth trail samples a requested **365.25 TT-day display window**, centered on the date when it is generated. Samples are separated by at most **0.25 TT day**, with the exact final endpoint included. Straight display segments join those positions. This does not declare that Earth's perturbed orbit has an exact 365.25-day period, replace the orbit with a Kepler ellipse, or force the final point to meet the first. The sample locations come from the numerically integrated, observation-fitted [DE440 model described by Park et al.](https://ssd.jpl.nasa.gov/doc/de440_de441.html); the connecting polyline has its own finite sampling error and is not a certified continuous trajectory representation.

When the current simulation instant lies strictly inside one stored sample interval, the display splits that segment through the exact current-epoch Earth position from the ephemeris. This joins the shown path to the current Earth marker without moving the stored samples. No extra point is inserted at an existing endpoint or outside the dated window; the remaining joins are still finite straight chords.

The window is clipped at the application's supported 1850–2149 calendar boundaries and labeled when shortened. A cached trail retains its dated samples while playback runs forward or backward inside its bounds. Leaving those bounds rebuilds the trail; the current Earth and Moon positions always use the current simulation instant. Unavailable samples produce an error rather than extrapolation or invented closure. The current clock's pre-1960 modeled-UT1 convention, future leap-second assumption and orientation-quality labels still apply; see [astronomy.md](astronomy.md).

## Instantaneous orbital plane

The displayed plane uses the **Earth center's instantaneous heliocentric osculating plane**, whose oriented normal is:

```text
n = (r_E × v_E) / |r_E × v_E|
u = r_E / |r_E|
w = normalize(n × u)
plane coordinates = α u + β w
```

The code normalizes position and velocity before the cross product to avoid overflow; this preserves its direction. Zero, nonfinite and numerically degenerate states are rejected. The normal points in the prograde angular-momentum direction. This definition includes Earth–Moon mutual motion because the target is Earth's center, not the Earth–Moon barycenter. JPL explicitly notes that choosing a planetary center instead of its system barycenter includes short-period local motion in osculating quantities. It is not the IAU mean ecliptic or a fixed J2000 ecliptic. [NAIF's osculating-elements definition](https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/cspice/oscelt_c.html) describes the relationship between an instantaneous state and its osculating geometry; this application uses the plane only and does not propagate a two-body orbit from it.

The local plane annotation is a circle centered on Earth, visible from the current physical eye. Its mathematical plane also contains the Sun because `r_E` lies in that plane by construction; there is no separate plane patch drawn at the Sun. The dated Earth path supplies the whole-orbit context. The circle is an inspection annotation, not a material surface, a collision target or a measured extent of the solar system.

## Earth orientation and tilt

The globe's body-to-celestial orientation is the transpose of the existing celestial-to-terrestrial matrix. It includes the same modeled bias/precession/nutation, UT1 rotation and available IERS polar motion as the first-person Earth. The source orbit packet also contains geometric Sun/Moon orientations, but the actual displayed bodies retain the existing apparent receive-time centers and retarded text-PCK orientations. Simultaneous orbit strokes are annotations, not replacements for these eye-dependent visible bodies.

The axis annotation follows the **modeled celestial intermediate pole (CIP)**. It is the third row of the adopted IAU 2006/2000B bias-precession-nutation matrix, before terrestrial spin and polar motion. This is the pole convention documented by [ERFA's `bpn2xy`](https://raw.githubusercontent.com/liberfa/erfa/v2.0.1/src/bpn2xy.c); [`c2ixys`](https://raw.githubusercontent.com/liberfa/erfa/v2.0.1/src/c2ixys.c) documents its place in the celestial-to-terrestrial transformation. The same existing ERFA-derived model and retained license apply. The CIP is a conventional orientation pole, not a recovered instantaneous angular-velocity vector. Polar motion also means it is not identical to the WGS84 body-fixed geographic north axis.

With normalized CIP vector `p` and orbital normal `n`, the numeric tilt is:

```text
tilt = atan2(|p × n|, p · n)             # directed normal-to-pole angle
axis angle above plane = |90° − tilt|   # acute line-to-plane angle
```

Both are labeled so the familiar roughly 23-degree normal-angle is not confused with the roughly 67-degree line-to-plane angle. The actual values change with the model and date; no constant tilt is imposed. The printed digits describe numerical evaluation and display rounding, not an all-date accuracy bound. Omitted observed celestial-pole offsets, subdaily orientation corrections and the wider-date limitations of the abridged nutation model remain as documented in [astronomy.md](astronomy.md).

## Display scope

The overview preserves physical radius-to-distance ratios: WGS84 Earth's two semiaxes, the adopted nominal Sun sphere and the mean-radius Moon sphere are not enlarged to fill the screen. Earth is consequently very small at the scale of its annual path. An identifying annotation and the **Aim at Earth** or globe-fit controls make it findable; annotation dimensions do not change physical body size. Orbit, plane and axis strokes are inspection annotations with finite pixel width.

The same active Earth surface, source category hierarchy and optional terrain remain selected during flight. The global Natural Earth context is generalized cartography, not photographic imagery or a surveyed coastline. Regional detail, grass, survey objects and atmospheric refraction retain their existing finite visibility and operational limits. Internal floating-origin/LOD changes do not alter the authoritative pose or physical geometry. GPU rasterization still has finite numerical and display resolution. The exact shader/camera implementation and its independently reviewed limits are described in [rendering.md](rendering.md).

Independent checks use the original CSPICE state evaluator and original ERFA pole routines, not a roundtrip through the production code. [`verify_orbital_reference.py`](../../scripts/verify_orbital_reference.py) reproduces the reference values from the existing SHA-pinned kernel without downloading another dataset. The review also checks plane orientation, time-window endpoints, source-centered trail sampling, missing/invalid states and the finite coordinate, fit and projection limits. Numerical agreement with these cases does not eliminate ephemeris uncertainty or certify every possible date and display view.

With the pinned optional [astronomy verification dependencies](../../scripts/requirements-astronomy.txt) installed under `.cache/python`:

```powershell
$env:PYTHONPATH = "$PWD/.cache/python"
python scripts/verify_orbital_reference.py --check
cargo test -p earth-math --test orbital_review
```
