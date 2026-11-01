# Source-backed lunar appearance and axis

The Moon retains the apparent center, 1,737,400 m mean radius, emission-time body attitude and center-dot sphere intersection from the [astronomical model](astronomy.md). NASA maps replace the former invented color bands. Normal, telescope, fisheye and active weather-refraction views share the same coordinates and material function. Relief changes shading only, not silhouettes, occultations, collision or reported lunar ranges.

## Maps and registration

The [NASA Scientific Visualization Studio CGI Moon Kit](https://svs.gsfc.nasa.gov/4720/) supplies the 2019 8192 by 4096 color image and the 5760 by 2880 LOLA elevation image. Credit: NASA's Scientific Visualization Studio. Source/output hashes, URLs and processing are recorded in `assets/lunar/provenance.json`; redistribution terms are in `licenses/NASA-SVS-lunar.md`.

NASA describes the color product as optimized for visual appearance. It combines LROC bands, adjusted exposure and white balance, small inpainted data gaps, and a monochrome laser-albedo polar fill. It is real mapped lunar geography, but its RGB values are not calibrated reflectance or an observation at the application's current date. No terrain/color features are generated procedurally.

Coordinates are planetocentric latitude and east-positive longitude. The full equirectangular map spans longitude -180 to +180 degrees left to right and latitude +90 to -90 degrees top to bottom. For body-frame unit direction `n`, `u = (atan2(n.y,n.x)/(2*pi)+1/2) mod 1`, `v = 1/2-asin(n.z)/pi`. Pixel centers are `(column+1/2)/width,(row+1/2)/height`. Sampling repeats longitude and clamps latitude. The cartographic mean-Earth source is placed using the existing IAU_MOON text-PCK orientation. This is a finite registration approximation, not an exact DE421/DE440 mean-Earth-frame transform. [NAIF describes more accurate binary lunar orientation models and required frame kernels](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/aareadme.txt).

The static display mesh has 384 longitude and 192 latitude segments. Its body axes are rotated into the committed local frame; shader dot products with those same axes recover body coordinates. Time-dependent orientation and libration come from the existing text PCK at the retarded Moon emission epoch.

Narrow fisheye fields adapt their forward perspective source to the selected FOV, then recover the unchanged equidistant rays during compositing. This prevents a telescope-scale lunar disk from being enlarged from only a few fixed-90° source pixels; it does not sharpen or invent lunar features. The shared [source-projection description](rendering.md#shared-world-optics) specifies the finite sampling and rounding limits.

## Relief normals and filtering

The unsigned LOLA TIFF stores half-metre counts with a 20,000-count offset: `h = 0.5*DN - 10000 m`, relative to the 1,737,400 m sphere. This follows the actual [SVS product documentation](https://svs.gsfc.nasa.gov/4720/). For `p=(R+h)n`, longitude lambda, latitude phi, east tangent `e_lambda` and north tangent `e_phi`, the outward unit normal is proportional to:

`n - h_lambda/((R+h)*cos(phi))*e_lambda - h_phi/(R+h)*e_phi`.

Derivatives are metres per radian. Preprocessing uses centered angular differences at source pixel centers, periodic longitude, and geographic continuation across each pole by reflecting the row and shifting longitude by pi. No source pixel is exactly at a coordinate pole; the public analytic helper uses a radial fallback within the tiny abs(cos(latitude)) < 1e-12 coordinate-pole band. No multiplier exaggerates slopes. Tangent vectors are normalized and stored in linear RGBA8. This is finite-difference and quantized shading, not a physical terrain surface or crater self-shadow solver.

Color mips average linear-light RGB then re-encode sRGB. Normal mips average tangent vectors and renormalize: they are presentation filters, not new measured terrain derivatives. Runtime sampling uses complete mip chains, longitude-repeat, latitude-clamp and bilinear/trilinear filtering. Fragment normal derivatives select mesh-view detail. The refracted view uses a local spherical footprint estimate from range, focal scale and incidence; it is not a bound on a refractive or caustic image Jacobian. Source resolution and finite screen sampling limit close-up detail.

## Illumination

Both paths use the existing Moon-to-Sun direction with sunlight retarded to the lunar emission event. For mapped normal `N`, direction to Sun `s`, and direction toward the eye `o`, set `mu0=max(N dot s,0)` and `mu=max(N dot o,0)`. On the lit, visible hemisphere, the display uses the normalized Lommel-Seeliger factor `2*mu0/(mu0+mu)`; otherwise it is zero. This single-scattering disk function has a physical basis but is not a fitted lunar photometric model. See the [primary disk-scattering formulation](https://doi.org/10.1051/0004-6361/201526456). A mean-sphere backside gate prevents mapped normals from lighting the body's far hemisphere.

The source display RGB is multiplied by this factor, with an additive linear RGB 0.006 dark-side floor. That floor is artistic readability, not computed Earthshine. Absolute radiometry, opposition surge, lunar eclipse shading, local crater occlusion, wavelength-dependent scattering and atmospheric extinction are not solved. In the weather pass the viewing direction is the vacuum continuation from the traced exit point; the finite lunar surface hit and body normal come from that same ray.

## Pole, orbit plane and tilt

At the Moon's emission TDB time, obtain geometric Moon-minus-Earth position `r` and velocity `v` from the existing DE440 reader. The instantaneous orbital normal is `normalize(r cross v)`; the radial axis is `normalize(r)` and in-plane tangent is `normal cross radial`. The lunar north pole is the text-PCK body `+Z` axis at the same epoch. All directions are rotated into receive-time ECEF. Tilt is `atan2(length(pole cross normal),pole dot normal)`, in degrees. This cartographic orientation pole is not an exact instantaneous angular-velocity axis.

The label names the **lunar osculating orbit normal at the emission epoch**. Its angle to the pole is around 6.7 degrees at contemporary dates and varies; it is not the roughly 1.5-degree ecliptic obliquity. The angle above the orbital plane is the absolute complement to 90 degrees.

The diagram shares the apparent lunar center, complete camera triad, lens, physical framebuffer aspect and logical UI scaling, including roll and fisheye clipping. A stale-eye sample or canonical Earth/terrain hit before the lunar center-ray surface suppresses the whole diagram. This conservative center-visibility policy can hide the diagram while a limb remains visible; remaining strokes are overlays, not individually depth-tested geometry. Plane radius is at least 1.35 lunar radii and may be enlarged for readability (45 physical pixels in the centered angular-scale estimate); pole and normal strokes extend 1.3 times that radius. Lines are direction annotations, not surface features or a lunar trajectory. Enlarged diagrams are labeled; measured captions use the shared bounded layout. The straight-ray diagram is omitted during weather refraction; the source-backed lunar surface still uses the curved-ray path.
