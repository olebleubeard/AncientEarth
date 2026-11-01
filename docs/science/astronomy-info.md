# Earth, Sun and Moon information

The **I** panel reports adopted model dimensions and current astronomical quantities in separate Earth, Sun and Moon tabs. [`astro/info.rs`](../../crates/earth-math/src/astro/info.rs) supplies every quantity and unit conversion; the interface only displays its rows, notes and source links. The report is constructed only while the panel is open. It reuses the current astronomical frame and does not run another ephemeris, optical solver or terrain query.

## Adopted dimensions

Earth uses the [NGA WGS84 defining geometric constants](https://earth-info.nga.mil/index.php?dir=wgs84&action=wgs84): equatorial semiaxis `a = 6378137 m` and inverse flattening `1/f = 298.257223563`. Their decimal definitions are exact within this reference system; their derived binary floating-point representations are rounded. The report evaluates:

```text
f = 1 / 298.257223563
b = a (1 − f)
e² = f (2 − f)
equatorial diameter = 2a
polar diameter = 2b
equatorial circumference = 2πa
meridional circumference = 4 × WGS84 geodesic distance from equator to pole
```

The meridian is an ellipse, so its circumference is not `2πb`. The implementation obtains its quarter-length using the existing reviewed [WGS84 inverse geodesic](../../crates/earth-math/src/geodesic.rs). These dimensions describe the reference ellipsoid, excluding downloaded terrain, geoid undulation and tides. Eye-to-Earth-center distance is the norm of the current ECEF eye position; eye height is its stored WGS84 geodetic height, not clearance above terrain or height above a sea-level datum.

The adopted Sun radius is `695700000 m`, the exact **nominal conversion constant** defined by [IAU 2015 Resolution B3](https://www.iau.org/common/Uploaded%20files/IAUGA2015-Resolution-B3-recommended-nominal-conversion.pdf). This is not a claim that the instantaneous solar photosphere has that exact physical radius. The Moon uses the mean spherical radius `1737400 m`, consistent with [NASA/JPL's satellite physical parameters](https://ssd.jpl.nasa.gov/sats/phys_par/). The lunar mean radius is an adopted approximation to a nonspherical, topographic body, not an exact measured limb. Both displayed spheres use:

```text
diameter = 2R
great-circle circumference = 2πR
```

Those expressions describe each adopted sphere; no new solar or lunar shape model is introduced.

## Live distances and angular size

The app's [JPL DE440s ephemeris and apparent-position calculation](astronomy.md) retain distinct simultaneous and apparent centers. The panel preserves that distinction rather than naming every distance simply “distance to the Sun/Moon.” For ECEF eye `O`, simultaneous body center `C`, and existing apparent model center `C_app`:

```text
Earth-center to body-center distance = |C|
simultaneous eye-to-center distance = |C − O|
apparent model eye-to-center distance = |C_app − O|
unrefracted apparent model angular diameter = 2 asin(R / |C_app − O|)
```

The first two use the common reception epoch. The apparent center includes the existing iterated reception light-time solution and observer aberration. The reported center light travel time comes directly from that solution; it is not obtained by dividing the simultaneous distance by light speed. [NASA/JPL's ephemeris description](https://ssd.jpl.nasa.gov/planets/eph_export.html) explains the underlying polynomial representation and TDB time argument; [astronomy.md](astronomy.md) documents this application's observer, frame and light-time conventions.

The angular formula follows the tangent from an external eye to the adopted sphere. It is unavailable on or inside that sphere. The app aberrates the center direction while retaining the adopted spherical radius; this is not a full relativistic extended-disk transformation. The quantity excludes atmospheric refraction, limb relief and the display's finite pixel resolution. It remains available when the body is hidden by Earth or terrain: this panel describes body geometry, not visibility. The center-dot surface range and optional [curved-ray optics](refraction.md) remain separate measurements.

The lunar illuminated fraction reuses the frame's geometric phase. With unit Moon-to-Sun direction `s` and Moon-to-observer direction `o`, it is `(1 + s·o)/2`. Evaluation occurs at the lunar emission event, with sunlight retarded to that event. It does not describe simultaneous reception-time illumination, measured brightness, albedo or photometric opposition effects.

## Current sample and units

Every sampled astronomical frame records its exact `AstroInstant`, ECEF eye position and Earth-following/nonrotating convention. The panel accepts live body quantities only when all three match the current request, the observer is valid, the adopted radii agree and the required scalar values are finite and within their domains. A look-direction change alone leaves these scalar quantities valid. A changed clock, eye position, posture or [F9 motion convention](reference-frames.md) invalidates the old sample. Missing, mismatched or invalid frames show unavailable/updating text and retain only adopted dimensions and independently valid current Earth-eye quantities; they never retain old live body values.

The epoch and quality text come from the same clock/frame, including the modeled-UT1 convention before 1960, measured or predicted Earth-orientation coverage, unknown future leap seconds and the abridged nutation interval. The application's supported calendar remains 1850–2149; the information panel does not extend it or imply uniform accuracy throughout it.

Lengths show kilometres, metres, international miles and international feet together, independent of the HUD unit selection. [NIST's exact conversion factors](https://www.nist.gov/pml/us-surveyfoot/revised-unit-conversion-factors) give `1 ft = 0.3048 m` and `1 mi = 1609.344 m`. Lengths are rounded to three decimal places, angular diameters to nine decimal degrees and six decimal arcminutes, and light travel times to six decimal seconds. These are display choices, not uncertainty estimates or statements of physical accuracy.

Independent review tests in [`astronomy_info_review.rs`](../../crates/earth-math/tests/astronomy_info_review.rs) check adopted dimensions, units, meridional circumference against an independent numerical integral, live quantities against independent ephemeris fixtures, angular edge cases and current-sample invalidation. Full astronomy sources and model limits remain in [astronomy.md](astronomy.md).

The current receiver convention is included in the common live-status text and Earth information rows. Runtime inspection uses actual-eye reception position and parallax with Earth translation plus the WGS84 h=0 observatory-spin reference at the same geodetic latitude/longitude; F9 uses translation alone. This bounded velocity reference is continuous at every flight height and is not the derivative of the virtual camera trajectory. The scientific raw sampler remains distinct. See [continuous-flight.md](continuous-flight.md) for the explicit instantaneous-Earth and center-only apparent-body image limits.
