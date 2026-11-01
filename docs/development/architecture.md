# Architecture

## Components

| Location | Responsibility |
| --- | --- |
| `crates/earth-math` | f64 WGS84 geodesy, analytic ellipsoid rays, geographic boundary classification, projections, distance, flight, units and astronomy; GeographicLib supplies inverse geodesics |
| `src/surface.rs` | Bundled height-free boundary loading, appearance selection and source labels |
| `src/datum.rs` | Offline EGM2008 datum comparison only; never changes the h=0 scene geometry |
| `src/navigation.rs` | Renderer-independent GPS navigation and command validation |
| `src/survey.rs`, `src/desktop/survey.rs`, `src/desktop/ui/survey_panel.rs` | Marker confirmation, tape interaction state, cancellable placement/occlusion jobs and survey controls |
| `crates/earth-math/src/survey.rs`, `rendering/survey.rs` | Reviewed marker placement, ray selection, tape/occlusion measurements and authoritative display geometry |
| `src/desktop/renderer/survey.rs`, `src/desktop/shaders/survey.wesl` | Marker mesh/material lifecycle; the GUI draws projected tape annotations |
| `src/desktop/mod.rs` | Bevy setup and screenshot mode |
| `src/desktop/controls.rs` | Bevy input translation and GUI capture policy |
| `src/simulation.rs` | Bundled ephemeris/Earth-orientation loading, clock and astronomy snapshots |
| `crates/earth-math/src/astro/eclipse.rs` | Spatial solar-eclipse model and readout formatting |
| `crates/earth-math/src/astro/reference_frames.rs` | Observer frame tracking and rotation-related clearance checks |
| `crates/earth-math/src/guides.rs`, `geodesic.rs` | Ellipsoid horizon guides and reference-surface distances |
| `src/desktop/renderer.rs` | Native GPU meshes, floating origin, celestial visuals, grass, lights and optical camera views |
| `src/desktop/renderer/materials.rs` | Original ground/water PBR extensions, eclipse uniforms, bounded GPU grass geometry and lunar material |
| `src/desktop/renderer/refraction.rs`, `src/desktop/shaders/refraction.wesl` | Native-pixel curved viewing rays, canonical terrain/prism event traversal, ellipsoid fallback and source-boundary lookup |
| `src/desktop/renderer/refraction/compute.rs`, `primary.rs`, `stellar.rs` | Current-frame optical dispatch, native terminal buffers and shared stellar-solution scheduling |
| `src/desktop/renderer/refraction/readback.rs` | Optional bounded QA readback; excluded from ordinary rendering |
| `crates/earth-math/src/atmosphere.rs`, `atmosphere/gpu.rs` | Validated phase-index profiles, f64 optical integration/inverse shooting and f32 local BVH/scene packing |
| `src/weather.rs`, `src/weather/transport.rs` | Optional weather acquisition, profile eligibility, cache and visible provider status |
| `src/desktop/frame_capture.rs` | Opt-in native frame pacing capture with configuration metadata |
| `src/desktop/shaders/fisheye.wesl` | Original equidistant reprojection; five sampled cube views, or one for narrow fields |
| `src/desktop/shaders/ground.wesl`, `grass.wesl`, `water.wesl`, `moon.wesl` | Filtered ground/water detail, shared grass color/shadow deformation and lunar phase lighting |
| `src/desktop/shaders/eclipse.wesl` | Shared spatial Sun/Moon occultation, embedded into ground, grass and water shaders |
| `crates/earth-math/src/rendering.rs`, `rendering/boundaries.rs`, `rendering/grass.rs` | Audited display tessellation, shared region edges, grass placement and sight intersection |
| `src/desktop/ui.rs` | Dear ImGui navigation, clock, presentation controls and sight HUD |
| `assets` | Offline boundary and astronomical data, provenance and license notices |
| `scripts` | Reproducible asset preparation, verification and packaging |
| `docs/science` | Scientific definitions, primary sources and review evidence |

## Precision and surface model

With elevations off, the physical Earth surface is the analytic WGS84 ellipsoid at `h=0` everywhere, including land, ocean, lakes and rivers. Runtime boundary assets retain geographic land/water partitions but contain no topographic heights. They do not displace the surface. The reference ellipsoid is an idealized model, not local mean sea level, a geoid or real topography. The separate EGM2008 readout remains a datum comparison at the observer's location; it does not move land, water, roots or the horizon.

Scientific geography, optical ray queries, collision and astronomy live in `earth-math`. Mathematical presentation formulas are also implemented by a mathematics subagent and independently reviewed. Dear ImGui draws controls over the GPU view at native framebuffer resolution.

Scientific measurements intersect the analytic f64 ellipsoid and classify the resulting geographic point using the retained boundaries. GPU display triangles are finite chords of that ellipsoid. Near Earth, an origin near the observer is subtracted from ECEF positions in f64; only the resulting local east/up/south coordinates are converted to f32. Movement requests a new local origin when needed. At an eye height of 2,000 km the next coherent display uses a fixed near-Earth coordinate anchor, retained until descent below 1,000 km. The actual Observer and full camera/lens never change at this anchor transition. Reference tessellation uses the actual eye independently of the coordinate origin, with a conservative prepared movement neighborhood and focal budget. Source geometry and geographic material uniforms remain near Earth rather than storing AU-scale offsets in every surface vertex. Scientific queries remain independent of display LOD and visual materials.

One Earth presentation worker prepares joint regional/reference meshes or imported canonical meshes, including Bevy mesh conversion and categorical-buffer packing. Camera transforms and FOV update immediately against the currently committed local frame. Finished same-scene snapshots commit even after further movement, then the latest request starts; intermediate camera requests do not accumulate or repeatedly cancel useful work. Regional/imported faces are grouped into spatial chunks of at most 16,384 faces with outward-rounded AABBs; their oriented geometry and vertex attributes are unchanged. This restores per-region view/shadow culling. All Earth meshes, chunk entities/bounds and their origin swap together, followed by corresponding material, camera, celestial, survey and grass updates in the same system. A physical-scene change hides incompatible Earth/grass geometry until current-scene presentation arrives. GPU uploads still occur on commit, and pending work is included in frame-capture readiness and telemetry. See the coordinate-precision and deferred-LOD limits in [rendering](../science/rendering.md).

Regional material meshes retain their full source-derived partition. All vertices use `h=0`, with the same analytic ellipsoid normal on both sides of a shoreline. The global reference mesh uses adaptive geographic tessellation. A joint builder unions regional perimeter posts with adaptive reference endpoints, then uses identical shared edge subdivisions on both sides. Regional outer faces that need new edge posts are fan-triangulated in their geographic parameter plane and projected to `h=0`; their material classes are unchanged. This removes region-edge T-junctions without overlapping skirts or height offsets. See [rendering mathematics and limits](../science/rendering.md).

The regional boundaries retain the source classification and bounded coastal reconstruction documented in [water reconstruction](../science/water.md). A shared fragment lookup gives these curves first priority, downloaded native Copernicus WBM posts second priority, and the original Natural Earth cartographic polygon boundaries last priority. The global vector source has a bounded adaptive spatial index; index-cell borders do not become coastlines. Exact native categories use bounded integer row-run storage and a tile lookup, rather than additional ECEF meshes or padded texture arrays. Missing native tiles are not assumed to be ocean. Both smooth and imported surfaces use these categories without changing geometry, and downloaded masks remain available with elevations off. Mixed and water regions suppress grass. No photographic imagery is loaded. Manual Copernicus downloads build a canonical terrain scene in a single background worker. `WorldSurface` switches immutable scenes and increments a generation counter; measurement jobs invalidate on that generation. Physical-terrain heading skyline and L/V computations run in cancellable workers. Eligible refraction intersects the same imported faces and masks the reference fallback only under actual canonical coverage. Source registration and import budgets are documented in [topography](../science/topography.md). Download storage carries licenses/provenance and an asset inventory; completed files survive toggles and cancellation.

Application frame pacing defaults to 60 foreground FPS, with 30/60/120/Unlimited choices and lower background/minimized targets. Its monotonic gate preserves real elapsed time and never catches up by issuing a burst of frames. The opt-in unrestricted QA mode bypasses these caps while retaining the same presentation/VSync policy. Frame recordings distinguish pacing, actual intervals and Bevy visibility eligibility from GPU execution measurements.

## Appearance and animation

Ground detail is a shading-only PBR extension with periodically anchored ECEF coordinates, derivative filtering and finite near-detail range. Original grass roots use the exact ellipsoid and boundary exclusion policy. Its resting geometry and bounded wind coefficients are prepared in a background task; color and shadow shaders apply the same frame uniform and observer-dependent fade. Explicit culling bounds include every wind pose. Meshes, blade records and sight-query bounds swap together after checking scene, origin, quality and observer validity. The Moon retains a static mean-radius sphere, shared NASA color/LOLA-normal mip textures and an emission-time orientation/illumination packet. Both geometric and refracted paths use the same source registration and material; relief is shading only, with unchanged sphere ranges. See [lunar rendering](../science/lunar-rendering.md). Earth uses a global geocentric directional sunlight approximation at every flight height, while apparent Sun/Moon centers are sampled from the actual observer. Illumination is explicitly approximate rather than a view-direction-dependent reversal of the light.

Water uses an opaque PBR extension with filtered periodic shading-normal ripples and an illustrative reflection of the original sky gradient. Only bounded phase uniforms change during animation. Water vertices, analytic range targets and the geometric horizon do not move. Colors, roughness, reflection and ripple frequencies are appearance choices, not measured local water optics or hydrodynamics. Finite shadow maps and MSAA apply to the fixed display geometry.

The astronomy snapshot supplies a receive-time solar-eclipse field. The renderer evaluates local solar visibility at every land, ellipsoid, grass and water fragment in a Sun-aligned coordinate frame. The observer's illustrative sky response is separate from this spatial attenuation and from nearby shadow maps. Shared shader source prevents surface occultation formulas from diverging.

There is one continuous geometric world-camera path from ground to the finite four-AU navigation domain. The former independent analytic overview pass is no longer registered; its packet tests remain historical numerical fixtures. The orbital resource caches only the current ephemeris context and dated trail. At an angular-scale criterion, annotations project that geometric path/CIP/plane and the actual apparent body centers through the canonical Observer and selected Lens. The shared chord projector handles rectilinear, telescope and fisheye clipping; caption layout uses actual font dimensions. Optional Fit Earth/whole orbit commands deliberately reposition this same Observer and do not park a second pose. Full-year trail sampling is cached instead of repeated every frame. The single rendering path retains physical terrain, source categories and ordinary optical eligibility throughout navigation. See [continuous flight](../science/continuous-flight.md) and [rendering limits](../science/rendering.md).

An eligible active weather profile switches the renderer to a native-resolution curved-ray pass. It launches the selected lens directions and integrates the same fixed, scene-anchored profile used by CPU measurements. Canonical terrain triangles and survey prisms use bounded curved-segment event searches; analytic WGS84 h=0 remains the fallback outside actual physical coverage. Classification uses the same regional/native/global hierarchy without changing heights. Finite Sun/Moon rays continue from the atmosphere exit point and tangent. CPU sight, marker picks, hidden-height queries, tape image projection and L/V use the same current optical scene, with separate f64 numerical controls. `WeatherState::view_profile_for_scene(observer, scene)` checks current source/scene identity and the inclusive −10–20 km WGS84 eye-height domain; missing/ineligible weather or an out-of-domain eye selects explicitly labeled geometric mode while retaining weather data. With an eligible optical profile, scene preparation and capacity failures instead remain explicit Pending/Unsupported/Error display states; they do not show an old packet or silently choose a straight view. A bounded worker prepares immutable per-patch BVH buffers, accepted only for the current scene identity; small eye/marker buffers update separately. Grass is hidden in optical mode because its raster blades lack a curved-ray intersector. Geometric mode retains PBR cameras, grass and shadow maps. Fog and sky radiance remain artistic; ray bending does not establish a full scattering/cloud/photometry model. See [weather and refraction](../science/refraction.md) and [display precision](../science/rendering.md).

The executable embeds required data and software/font attribution. `--licenses` prints it without starting graphics. Source-reviewed asset hashes and the locked Windows dependency license inventory are checked before packaging; see [licensing and distribution](licensing.md).

## Frame order

The optical render graph dispatches primary rays before the composite camera. A
current submission carries its viewport and frame token with the associated
geometry and output buffers. When stellar sharing is active, three subsequent
compute passes clear requests, gather requested catalogue entries from those
primary results, and solve the requested entries. The compositor checks the
expected current token before consuming either table. These are same-frame
dependencies; camera movement never reuses a previous frame's solved view.
Missing extraction, assets or pipelines clear prepared bindings instead of
dispatching an old packet. A minimized window withdraws the submission. Compute
support is checked before output allocation; unsupported output extents retain
the direct shader path.

`Availability::Ready` means the render-world inputs and pipeline can be
submitted. It does not acknowledge hardware completion or presentation. The
optional `--optical-header-readback` diagnostic, restricted to a bounded
`--frame-capture`, records requested and returned primary/stellar headers,
including pending and mismatched responses. It is not enabled during ordinary
flight and is not a GPU timer. `--optical-no-stellar-cache` retains the original
stellar scheduling for regression comparisons; `--optical-direct` selects the
direct optical shader for QA. Numerical acceptance and measured performance
limits are recorded separately in [verification](../science/verification.md).

The Dear ImGui pass follows Bevy input collection:

1. GUI actions update GPS navigation, clock and lens settings.
2. Input handling applies permitted flight, look and zoom actions.
3. The clock updates the observer's selected reference frame and astronomy snapshot.
4. Survey jobs accept only matching scene snapshots, select the current-dot target and update the shared geometric-view override. Physical-surface changes clear the previous survey.
5. Weather eligibility is checked against the new observer location and real UTC; old-location profiles are removed before rendering. `view_profile_for_scene` also checks terrain and survey eligibility.
6. GPU presentation updates camera transforms, required display meshes, grass, survey geometry and celestial lighting, or the active curved-ray pass.
7. The HUD measures the same observer pose and draws the sight readout and optional guides. F2 hides panels and dot independently of L/V and survey captions.

Survey placement has one cancellable worker and no request backlog. Hover picking tests prism candidates first and queries the Earth only when a prism is hit. Hidden-height work has a separate latest-snapshot worker keyed by eye, marker and surface generation. Measurements never scan every marker's horizon. Changing tape endpoints or the row increments the survey generation; renderer meshes rebuild on that generation or a floating-origin change. Visibility toggles retain the immutable generated data. The physical rules and finite limits are in [survey geometry](../science/survey.md).

UI capture blocks gameplay input while editing fields or using panels. Focus loss releases the mouse. Crouch updates the math library's eye posture before rendering and measurement. F9 selects whether the observer follows Earth, transporting the complete camera pose before rendering. L/V guide measurements are independent of GPU LOD and drawn in physical framebuffer pixels. See [reference frames](../science/reference-frames.md) and [horizon guides](../science/horizon-guides.md).

The shared `OpticalSight` center trace, marker hidden-height, optical tape projection, expensive refracted guides and visible heading readout use bounded latest-snapshot workers in `src/desktop/ui/jobs.rs`. Each permits only one job in flight, replaces intermediate requests instead of queuing them, and accepts results only for the current snapshot. Guide work checks cancellation between ray queries. Camera, lens, physical viewport, active surface and weather generation invalidate guide results; column movement can reuse the current L profile. Pending measurements are labeled, never presented as current-pose values. Clean view skips the hidden heading readout. Screenshot and stationary-frame-capture warmup wait for requested measurements to finish.

## Dependencies and verification

Bevy `=0.20.0` and `dear-imgui-bevy` / `dear-imgui-rs` `=0.21.0` are pinned as a compatible release train. The lockfile pins the resolved tree. Rust 1.97.1 is the backend's minimum compiler. The math library has no rendering dependency and can be tested independently. Bevy manages ECS, input, window, GPU materials, shadow maps and antialiasing; Dear ImGui supplies the interactive panels. The baseline surface/astronomy scene operates offline; live weather is an explicit network-backed option, with provider and cached-data state visible to the user.

GPU pipelines compile asynchronously. `graphics_loading` publishes pipeline status after Bevy's render stage through a shared mailbox, without manually processing or blocking on the queue. An opaque startup screen covers partial geometry until the CPU scene is ready and three distinct render reports show a nonempty, stable-size, successful pipeline cache. Repeated reads of one report do not count as additional completed frames. Missing shader/import errors remain pending; permanent compiler failures are displayed. Flight and ordinary panels wait for the initial reveal, while the loading window can close the app. Subsequent pipeline work shows a compact status; mode switches may temporarily show incomplete frames. Driver initialization and uploads remain possible blocking work. Screenshot and stationary-capture readiness include this status; recordings retain every expensive interval after sampling begins.

Readiness also requires the pinned Dear ImGui backend's overlay pipelines. The optional `--capture-loading --screenshot path.png` QA route waits for those UI pipelines and captures while other graphics work is still pending; it does not insert an artificial loading delay. A very fast cache can therefore finish before this QA condition occurs. Ordinary captures always wait for completion.

When classified surface chunks own the Earth, retained legacy placeholder entities remain hidden: zero raster area alone does not prevent material-pipeline specialization. Disabled or empty grass also stays hidden. This is separate from the background Earth mesh worker: changing camera FOV does not synchronously retessellate the world.

Reference-mesh assembly binary-searches the sorted shared edge registry instead of scanning a whole geographic line for each cell, retaining identical post predicates and order. Fisheye cameras skip unsampled cube faces: the rear view is outside the supported aperture, and narrow fields use only the forward view. That forward camera follows the selected narrow FOV, with the same f32 projection scale in the compositor and stable small-angle ray evaluation; wide fields retain 90° source views. Target dimensions, depth testing and physical geometry are unchanged. Source focal length also drives terrain detail; see the projection and finite rounding limits in [rendering](../science/rendering.md).

Continuous-flight frame captures record renderer elapsed wall durations for polling, mesh publication and request submission, alongside worker duration/thread identity and UTC correlation. These are CPU-side measurements, not GPU execution times. The extra `--diagnose-pipelines` flag processes the public pipeline queue before the normal render-stage call to isolate synchronous pipeline creation and log considered descriptors. That diagnostic adds a queue-processing attempt and is excluded from ordinary before/after performance captures.

Run the scientific tests before desktop checks. Capture ground, coast, fisheye and telescope views after rendering or input changes. Numerical tests verify analytic intersections, classification, source registration and shared-edge display topology; screenshots separately check shader compilation, visibility and visual continuity. Neither establishes real-world survey accuracy for the idealized model.
