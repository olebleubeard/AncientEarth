# Body aiming and continuous camera tracking

The Earth, Moon and Sun toolbar controls rotate the existing camera. They do not move the feet or eye, change body positions/sizes, change the lens, or introduce another viewing mode. A single aim releases a previous tracking selection after the new aim succeeds. Selecting continuous tracking aims immediately and then updates after each current astronomical sample. Repeating the selected tracking target releases that constraint without changing the pose. The UI distinguishes single from double clicks before invoking these operations.

## Target and sample identity

Earth's target is the geometric ECEF origin. Sun and Moon use `AstronomyFrame.sun.apparent_center_ecef_m` and `.moon.apparent_center_ecef_m`, the same center packet used by their ordinary display and sphere-range queries. These include the existing reception-position light-time/parallax and declared inspection-reference aberration convention; they are not simultaneous heliocentric centers. [Astronomy](astronomy.md) and [continuous flight](continuous-flight.md) retain all ephemeris, Earth-orientation and extended-body approximation limits.

For eye position `P` and target center `T`, the requested forward direction is:

```text
F_new = (T − P) / |T − P|
```

Sun/Moon samples must match the exact simulation instant, actual ECEF eye position and Earth-following convention. Missing or mismatching samples never supply an old target. The selection remains active for recovery, while that update leaves the supplied camera pose unchanged and reports the unavailable condition. Earth aiming needs no astronomical sample. Invalid observer coordinates, nonfinite axes or an undefined zero-length target direction reject transactionally. Being inside an adopted sphere does not itself make a nonzero center direction undefined.

A target may be below the local horizon or behind an occluder; aiming does not remove that occlusion. In the optional refractive renderer, curved optical rays can shift the displayed body's image from this straight-axis center direction. This feature does not solve the atmospheric inverse-image problem or claim to keep every refracted or multiply imaged limb centered.

## Bank-preserving orientation transport

Let `F_old,U_old` be the current orthonormal forward and camera-up axes. Define `w = F_old × F_new`, `s = |w|` and `c = clamp(F_old·F_new,−1,1)`. When `s > 64 ε`, where `ε` is binary64 machine epsilon, rotate the old triad by the shortest proper rotation with axis `k=w/s` and angle `θ=atan2(s,c)`. Rodrigues' formula gives:

```text
U_rotated = U_old cos θ
          + (k × U_old) sin θ
          + k (k·U_old) (1 − cos θ)
```

For the numerically parallel or antiparallel case, retain old up. At an exact antipode this specifies the otherwise nonunique half-turn about old up. The new forward is always the actual normalized target direction; `set_view_axes` orthogonalizes the transported up against it and stores heading, pitch and roll. Thus the singular-band fallback does not approximate the target direction. It only chooses a deterministic bank convention. Successive small updates transport bank continuously through zenith, nadir and poles when horizon lock is inactive. The aiming algorithm itself does not impose a fixed local-horizontal roll; a closed directional path can accumulate orientation transport. The default [horizon lock](horizon-lock.md) subsequently overrides bank in its declared near-Earth eye-height domain while leaving the aimed forward direction and position unchanged.

Flight, posture changes, time playback/reversal and F9 continue to use their existing physical-position rules. The tracker aims from the resulting current eye. After success, the Earth-reference tracker is reanchored to the same position and new orientation at the sampled epoch, so nonrotating following does not restore old camera axes on the next update. Manual look, permitted roll and explicit landing/navigation commands release body tracking. Q/E commands ignored by an active horizon lock do not release the target. Movement and lens changes do not. Tracking is a kinematic viewing constraint, not a mount dynamics, spacecraft attitude or physical angular-momentum simulation.

## Screen controls and symbols

Layout is computed in integer physical framebuffer pixels, then returned as logical ImGui rectangles. Visible side-panel handles attach immediately outside the inner edge: right edge of the left panel and left edge of the right panel. The bottom handle attaches above its panel. If a panel is collapsed or cannot meet its usable size, its handle returns to the corresponding screen edge. Nominal handle sizes are 16×36 logical units for the sides and 36×16 for the bottom; content and handles have disjoint hit rectangles. Tiny viewports keep bounded controls where possible, without claiming text legibility.

A reserved top row holds a 176×34 logical-unit toolbar. It is omitted when it cannot fit or would intersect another provided panel/handle. Four button rectangles divide its physical width with finite gaps; the fourth controls horizon roll lock. The Earth disk/meridian, Moon crescent and Sun disk/rays are original decorative outline symbols, generated in physical-pixel coordinates and converted to logical coordinates for drawing. They are not maps, observed angular sizes or the current lunar phase. The separate lunar scientific information, if shown, uses its own astronomical calculation.

Implementation: [`astro/aim.rs`](../../crates/earth-math/src/astro/aim.rs), [`rendering/layout.rs`](../../crates/earth-math/src/rendering/layout.rs), and the post-sample application sequencing in [`simulation.rs`](../../src/simulation.rs).
