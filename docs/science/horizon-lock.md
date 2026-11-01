# Local-geodetic horizon roll lock

Horizon lock is a camera-control constraint, enabled by default. It removes sideways camera bank near Earth while retaining the current heading, pitch, forward sight direction, eye position, feet position, lens, speed and posture settings. It does not flatten the Earth, move the horizon to screen center or straighten an observed terrain skyline. Curvature, horizon dip, source elevations, occlusion and optional atmospheric bending remain in their existing models. The fourth top-toolbar symbol and the Observer & Display setting control the same flag; this is separate from the L/V measurement overlays.

## Active domain and boundary behavior

The lock is active for a valid canonical observer when enabled and the **WGS84 ellipsoidal eye height is at most 100,000 m**, inclusively. That is 100 km above the reference ellipsoid, not height above terrain, the EGM2008 geoid or instantaneous sea level. The constant is a practical application control choice. It does not define a sharp physical edge of the atmosphere or the valid range of the weather/refraction solver. Negative valid ellipsoidal heights are included. Invalid coordinates, eye offsets or noncanonical/nonfinite orientation fields are left unchanged for the normal navigation validation to handle.

Above the boundary, free roll remains available even while the setting is enabled. Crossing back below it sets bank to zero at the next constraint update. Disabling the setting leaves the current orientation alone; it does not restore an earlier bank. No interpolation, hysteresis or artificial change to altitude is applied. Crouching or editing eye offset can cross the threshold because it is evaluated at the eyes. The accepted stored pitch interval is −90° through +90°, matching the ordinary camera and Earth-reference tracker. Full free-look canonicalizes its representation when passing through a vertical sight direction.

## Mathematical definition

Let `E,N,U` be the local WGS84 east, north and geodetic-up basis at the current latitude/longitude, `H` the stored heading, `p` the stored pitch, and `r` the bank angle. Define horizontal heading `D`, zero-bank right `R0`, forward `F` and zero-bank camera-up `V0`:

```text
D  = E sin H + N cos H
R0 = E cos H − N sin H
F  = D cos p + U sin p
V0 = U cos p − D sin p

camera_up = V0 cos r + R0 sin r
```

Enforcement changes only `r` to zero. Forward is independent of bank, so the stored heading/pitch and the computed forward direction are unchanged, including very steep sights. Away from zenith/nadir, `V0` is the normalized projection of geodetic up onto the plane perpendicular to forward. This uses the ellipsoid normal, not a measured gravity/plumb-line direction or a sloping triangle normal.

At exactly vertical sight, that projection is zero and there is no unique horizontal azimuth or upright screen orientation. The existing stored-heading ENU convention selects `R0,V0`; no heading or pitch is invented or clamped during leveling. A globally continuous upright frame through every zenith/nadir crossing is not mathematically available under this convention: heading coordinates can change by 180° across the singularity. The lock preserves the sight direction and uses the declared local frame, rather than claiming an ambiguity-free horizon at a vertical view. Turning the lock off retains unrestricted full-triad transport through those directions.

## Update order and interaction

While active, Q/E roll commands are ignored and **do not cancel body tracking**. Accepted mouse look still changes the sight direction and releases a tracked target; leveling follows the look update. Movement uses the current body axes with the existing normalized inputs and collision/integration rules, then the endpoint's local frame is leveled. Thus Space/Shift still follow body up/down, including pitch, rather than becoming a fixed global vertical control.

Earth-reference updates and body-target aiming are evaluated first. The lock has final precedence over bank only. Sun/Moon/Earth tracking retains exactly its newly computed forward direction and current eye position; local upright replaces its otherwise transported bank. The final complete axes are reanchored at the accepted simulation epoch so a subsequent F9 update cannot restore stale bank. F9 still constrains the eye and forward direction in the Earth-centered nonrotating frame, but active horizon lock deliberately releases the inertial camera-up constraint. Turn the lock off to preserve a fully inertial orientation. Existing time-change collision checks, rollback, and body-sample freshness requirements remain unchanged.

Explicit body aiming and coordinate/posture edits apply the same lock immediately where eligible. G still performs its distinct operation: place feet on the active surface and reset both pitch and bank to an upright, level view. Horizon lock alone never resets pitch or moves the feet.

## UI and verification scope

The top toolbar is 176×34 nominal logical units with four disjoint rectangles registered to physical framebuffer pixels. The horizon symbol is an original circle and level bar using the same bounded physical-coordinate construction as the body icons. Its straight line is a control symbol, not a depiction or measurement of Earth's horizon. Tiny viewports may omit the toolbar; the setting remains available when its panel can be displayed.

The repeatable native comparison preset `--horizon-lock-demo` starts at latitude/longitude zero, 50,000 m feet height, heading 90°, pitch −10° and bank 30°. Enabled/off captures keep that physical pose, time and lens; only the permitted bank constraint differs. It is a diagnostic camera, not a surveyed site or physical aircraft trajectory. The existing one-million-metre rolled guide preset remains outside the lock domain.

Implementation: [`navigation.rs`](../../crates/earth-math/src/navigation.rs), application [`navigation.rs`](../../src/navigation.rs), [`simulation.rs`](../../src/simulation.rs), and [`rendering/layout.rs`](../../crates/earth-math/src/rendering/layout.rs). Independent tests and final native evidence are recorded in [verification](verification.md).
