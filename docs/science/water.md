# Height-free land/water boundaries on WGS84

In smooth mode the application places **all land, ocean, lake and river surfaces on the analytic WGS84 ellipsoid at ellipsoidal height h=0**. Geographic categories combine reconstructed regional Copernicus borders, downloaded native Water Body Masks and coarse global Natural Earth context, in that priority order. No DSM heights, geoid displacement, bathymetry or photographs enter classification preparation. Optional [downloaded elevations](topography.md) independently replace the physical surface in covered regions; the categories color that source-height mesh without flattening water. Historical bundled elevation assets remain archived.

## Native downloaded masks and global context

The earlier display used only four approximately 20km regional category crops, even when a much larger DEM download was active. Their material edge could appear as a rectangular island in a uniform surrounding color. The category and elevation footprints are now independent: official matching WBM files accompany the manual tile download, and a global cartographic layer covers areas without native masks. Source absence and a DEM value of zero are never interpreted as ocean.

The strict native reader accepts original UInt8, one-band, DEFLATE/predictor 2 GeoTIFFs with EPSG:4326 degrees and PixelIsPoint registration. It checks dimensions, angular steps, source tie point, category domain 0–3 and any nodata declaration. At tile southwest index `(S,W)`, source row zero is latitude `S+1` and column zero longitude `W`. There are 3,600 latitude rows; native longitude widths follow the provider's latitude bands. Raw TIFF tie points are point positions, so the reader does not add GDAL's half-pixel correction a second time.

Lookup rounds latitude to the nearest integer arcsecond `p=floor(latitude×3600+0.5)`. The owning tile is `S=floor((p−1)/3600)`, and row is `(S+1)×3600−p`. This ownership is evaluated before longitude spacing, including at 50°/60° band changes. With native width `w`, half-arcsecond longitude step is `d=7200/w`; select `k=floor(longitude×7200/d+0.5)`, wrap `k×d` to `[-180×7200,180×7200)`, then derive the tile and column by Euclidean division. Half-post ties choose north/east. An absent east/south owning source is unavailable rather than clamped or duplicated. This is nearest **angular source post** classification, not a geodesic-nearest measurement or added shoreline detail.

Each row is encoded as runs `(exclusive_end_column<<2)|category`, with row offsets. Decoding one native file at a time bounds temporary memory, and a total 16-million-run limit bounds retained mask storage to 64 MB of runs plus indices. The encoding preserves every source class; it is not a coarser mask. Binary-search lookup and integer GPU indices share this registration. The original reconstructed AEB borders described below remain higher priority, including unresolved class 4, so their declared close-up curves do not change.

Global context comes from [Natural Earth land v5.1.1, scale 1:10 million](https://www.naturalearthdata.com/downloads/10m-physical-vectors/10m-land/), whose [actual terms](https://www.naturalearthdata.com/about/terms-of-use/) place these data in the public domain, including commercial modification and redistribution. The original archive is SHA-pinned in provenance. `prepare_global_vector.py` reuses the strict original ESRI Polygon reader following the [published format](https://www.esri.com/content/dam/esrisites/sitecore-archive/Files/Pdfs/library/whitepapers/pdfs/shapefile.pdf), checks WGS84 registration, and retains the original directed line segments in longitude/latitude coordinates. It does not interpret an edge as a great-circle arc or wrap it to the shortest longitude difference.

The former global derivative sampled 0.01° cells, which could expose large rectangular shoreline steps in orbital views. The current runtime instead evaluates the original vector boundary. A 360×180 array of one-degree root cells indexes the world. Each cell subdivides into southwest, southeast, northwest and northeast children until a leaf contains at most 32 source-edge fragments. Clipping creates endpoints only on the original segments; neither cell borders nor new smoothing curves become coastlines. A strict depth limit of 20 and a bounded GPU word allocation reject an unrepresentable preparation instead of dropping segments or reverting silently to pixels. The supplied index needs only nine levels below a root.

Every leaf has a strictly interior seed `S`, selected from exactly binary-representable local coordinates and checked against the original polygon winding. Clockwise land exteriors contribute negative winding; counterclockwise enclosed-water holes contribute positive winding. The separate hole winding distinguishes the source's enclosed inland water from exterior ocean. Summing directed winding, rather than global exclusive-or, also preserves land where source polygons overlap. For query `Q`, the straight segment from `S` to `Q` remains inside the convex cell, so only source edges intersecting that cell can change membership. With local edge `A→B`:

```text
D = Q−S; u = dot(D,D)
yA = cross(D,A−S); yB = cross(D,B−S)
tA = dot(D,A−S);   tB = dot(D,B−S)
if (yA>0) differs from (yB>0):
    t = (tA yB − tB yA)/(yB−yA)
    if 0<t<u: winding += sign(yA−yB)
```

Here `cross` is the signed two-dimensional determinant and `t` is the intersection's unnormalized coordinate along `D`. Hole edges update both winding counts. `Q=S` keeps the seed state. Negative total winding means land; otherwise positive hole winding means inland water, and the remaining exterior is ocean context. The strict segment interval and half-open edge test define finite boundary conventions. Queries exactly on a source edge can take either adjacent category within arithmetic uncertainty; this cannot change physical heights or Earth ranges. Query longitude alone wraps at the date line; the terminal root contains each pole. Constant leaves contain no source edges and return immediately.

The `AEGLND02` asset contains 113,420 nodes and 503,417 clipped fragments from 439,337 nondegenerate original segments. Seeds and endpoints are float64 normalized within their leaf; the GPU uses separately reviewed float32 local values and integer addresses. Minimum seed-to-edge clearance in this artifact is over 0.08 leaf units. The uncompressed artifact is 22,206,164 bytes; its GPU index is bounded independently. The old `AEGLND01` raster remains only as an archived regression asset.

This removes the added pixel staircase, not the source's cartographic generalization. The global context remains a 1:10 million map, not a surveyed shore, current tide, native DEM footprint or complete inland-water inventory. Lakes and rivers outside native WBM/AEB coverage may remain generalized land. Transitions between source products can still reveal their different detail and source epochs; the application does not invent surveyed detail to blend them. No datum conversion occurs in category lookup.

Global preparation is reproducible with the pinned optional processing environment:

```powershell
$env:PYTHONPATH = (Join-Path (Get-Location) '.cache/python')
python scripts/fetch_global_land.py
python scripts/prepare_global_vector.py --check
```

The original source archive must be present at `.cache/natural-earth/ne_10m_land.zip`. Its exact source URL, SHA-256, public-domain terms, modification recipe and derivative hash accompany `assets/boundaries/global-land.vector.provenance.json`. Downloaded WBM files retain their own Copernicus license and per-file provenance in the runtime cache.

## Sources, registration and rights

The [Copernicus product handbook, section 1.2.5.4](https://dataspace.copernicus.eu/sites/default/files/media/files/2024-06/geo1988-copernicusdem-spe-002_producthandbook_i5.0.pdf) defines `0=non-water`, `1=ocean`, `2=lake`, `3=river`. Categories are separate from the elevation band. Neither a zero height nor a blue material establishes real water coverage.

Four already reviewed WBM tiles are cropped using fixed source row/column bounds recorded in [boundary provenance](../../assets/boundaries). Each actual source is a 3600×3600 unsigned-byte EPSG:4326 north-up PixelIsPoint grid. The [AWS processing description](https://copernicus-dem-30m.s3.amazonaws.com/readme.html) describes shared-edge trimming; actual transforms and hashes are checked rather than inferred. The original categorical band is read without averaged overviews.

Rasterio/GDAL exposes a corner-based affine even for point rasters. Native post `(row,column)` is evaluated at `(column+0.5,row+0.5)`, with no additional half-pixel shift, following [GDAL RFC33](https://gdal.org/en/stable/development/rfc/rfc33_gtiff_pixelispoint.html). The retained base uses stride three, while ocean-boundary cells include native posts. The importer checks source SHA-256, CRS, registration, category domain, scale/offset and finite crop bounds. Its only raster input is WBM; DSM/geoid files are not opened.

The exact EULA was reviewed before use. Modification/redistribution rights and retained obligations are recorded in [the source review](../../licenses/water-sources.json), [original license](../../licenses/Copernicus-WorldDEM-30-eula_F.pdf), [NOTICE](../../NOTICE.md) and [asset inventory](../../licenses/assets.json). These permissions apply to the identified product files, not arbitrary public map downloads.

## Declared ocean-shore reconstruction

Each coarse quadrilateral covers a four-by-four native-post stencil. If ocean and another category both occur, it is refined into three-by-three native cells. This changes categorical interpolation only. Ordinary homogeneous cells remain coarse; unresolved inland-water mixtures retain class4 and do not imply land.

For an ordinary binary land/ocean cell, the category changes on exactly two of its four outer edges. Their midpoints define the reconstructed boundary endpoints. The cell's terrain diagonal is **not** an extra shoreline constraint: forcing a three-to-one category pattern through a diagonal midpoint created artificial right-angle steps in the earlier implementation. A checkerboard has four crossing edges and genuinely ambiguous connectivity; only that case keeps the declared fixed NW–SE split into two triangular contour parents. Binary triangles beside inland-category mixtures likewise retain their triangular parent.

At a crossing shared by two contour parents, consistently oriented incoming and outgoing unit chords define a common tangent direction. An open contour end uses its own chord. For endpoints `P0,P3`, each parent contains a cubic Bézier curve:

```text
B(t) = (1−t)³ P0 + 3(1−t)²t P1 + 3(1−t)t² P2 + t³ P3
P1 = P0 + h0 T0; P2 = P3 − h1 T1; 0≤t≤1
shared tangent T = normalize(unit incoming chord + unit outgoing chord)
```

Handle lengths are capped by one third of the endpoint chord and by the convex parent's half-planes. Their projections occupy the first and last thirds of that chord, respectively. Convex-hull containment keeps the entire curve within its source parent; monotone chord projection prevents a self-loop. Shared tangent directions produce geometrically tangent (`G1`) joins in source coordinates, rather than the former sharp staircase. Handle magnitudes need not match, so parameter derivatives are not claimed to be `C1`. The finite sampled curve retains numerical and tessellation limits. Source post positions and categories remain fixed; no source height is used.

De Casteljau subdivision continues until each subcurve's two interior controls are at most **0.0005 source-grid units** from its closed endpoint segment. Distance to a convex segment is convex, so this bounds the whole subcurve's deviation from that segment in source parameters. One source-grid step is one arc second here; this is a fine tessellation tolerance, **not** centimetre physical coastline accuracy or a certified ECEF error bound. Ear clipping triangulates the two resulting possibly nonconvex class polygons while retaining every sampled boundary vertex. Both materials share the identical ordered boundary vertices.

This is a declared smooth reconstruction from categorical samples, **not a surveyed shoreline or new subpixel information**. Native-scale scalloping, missing small features and source-position uncertainty remain. The finer mesh removes artificial angular teeth but cannot recover an exact beach from approximately 30 m categorical posts.

At refined/coarse boundaries, both sides use the identical ordered native/crossing samples. Transition fans use source-parameter barycenters, not Cartesian barycenters or elevation samples. Every vertex parameter maps to latitude/longitude and then to WGS84 ECEF at h=0. A curved geodetic surface and a Cartesian triangle chord are different: only the former is authoritative for physical queries.

Cells without ocean boundaries retain the conservative 16-post stencil. Both coarse triangles receive a category only when every native post agrees; otherwise class4 means unresolved. Narrow inland channels can therefore be absent at the base sampling scale. This is regional categorical coverage, not a global coastline or a history of shoreline movement. Astronomy time does not change the source epoch.

| Region | Ocean faces | Lake faces | River faces | Unresolved faces |
| --- | ---: | ---: | ---: | ---: |
| region-01 | 0 | 0 | 0 | 20 |
| region-02 | 59,304 | 6 | 270 | 1,951 |
| region-03 | 0 | 4 | 110 | 2,170 |
| region-04 | 237,483 | 929 | 154 | 3,492 |

## Height-free format and categorical query

`AEBNDR01` has a 56-byte little-endian header: magic, u32 base width/height, f64 west/north/coarse longitude-step/latitude-step, then u32 vertex/face counts. Each vertex stores only two f64 values, **local [row,column] in native WBM grid units**, with north-west origin and base spacing three. Each face stores three u32 indices and one category byte. There is no elevation field or hidden Cartesian height. Per-file provenance records source hashes, registration, curve parents/controls/subdivision and topology recipes.

For local parameters `(r,c)`:

```text
latitude  = north − r × coarse_latitude_step / 3
longitude = west  + c × coarse_longitude_step / 3
height    = 0
```

The exact extrema use the same stored bounds arithmetic as the surrounding display mesh. The parser rejects malformed lengths, unsupported geographic grids, nonfinite/out-of-range parameters, changed retained-grid coordinates, invalid class/index values, reversed/degenerate faces and faces crossing their registered coarse-cell boundary. Independent source validation additionally checks full manifold coverage and curve/class consistency.

After an analytic ellipsoid hit, its latitude/longitude converts back to source parameters. Oriented edge signs locate its class face in local coarse-cell bins. Neighbor bins and a 1e-8-native-unit edge tolerance handle floating-point roundoff; that tolerance is not a physical uncertainty. Shared-edge decisions conservatively prefer a non-grass class. Class4 remains unresolved. Outside these regional crops, lookup proceeds to native WBM and then the coarse source-backed global context described above. Exact boundary hits can carry either adjacent material category within numerical tolerance without changing physical position or range.

## Smooth physical surface versus presentation

`TerrainScene::boundary_only` accepts only height-free patches. Ground placement returns the exact input latitude/longitude at h=0. Earth range uses the analytic WGS84 quadratic; category lookup occurs afterward. Collision and heading-specific skyline use that same ellipsoid, and L/V guides and F9 clearance ignore category display triangles. Entered negative feet heights are rejected. Decorative grass roots are evaluated on the ellipsoid only where the declared category is land.

Rendering evaluates all category vertices at h=0, uses analytic ellipsoid normals, and shares regional perimeter subdivisions with the surrounding global mesh. These finite display chords and f32 rasterization are approximations to the smooth surface. They cannot change the f64 Earth range or physical horizon. The water material is opaque illustrative shading; normal ripples do not move vertices. There is no swimming, seabed collision, tide, wave displacement or measured vegetation. See [rendering.md](rendering.md).

The optional geoid display remains independent: `H_EGM2008=h−N`. In smooth mode, all modeled ground/water has h=0, so its displayed height above that datum is `−N`, generally nonzero. This intentional ellipsoid surface is **not** geoid sea level or the actual ocean. Optional downloaded elevations instead retain their separately converted source heights, as described in [topography.md](topography.md). The [NOAA geoid explanation](https://oceanservice.noaa.gov/facts/geoid.html) distinguishes the gravity reference; real water additionally varies with tides, currents and weather. No geoid grid is used by the boundary importer.

## Reproduction and independent review

Install [pinned optional requirements](../../scripts/requirements-terrain.txt), then fetch only the reviewed WBM sources. The existing cache environment can be used as follows:

```powershell
python scripts/fetch_water_sources.py
$env:PYTHONPATH = (Join-Path (Get-Location) '.cache/python')
python scripts/prepare_boundaries.py
python scripts/prepare_boundaries.py --check
python scripts/verify_boundaries.py --report .cache/boundary-verification.json
python -m unittest discover -s scripts -p test_boundary_geometry.py
```

The verifier independently reads WBM files and checks hashes, point registration, retained/native coordinates, parameter barycenters, categories, parent containment, tangent joins, flatness and shared curve sampling. Internal edges must pair oppositely, the patch has one perimeter, and source-parameter faces cover the registered rectangle without holes/overlap. It does not import preparation code or read DSM/geoid data. Rust checks compare analytic queries across every class and coverage edge, validate h=0 vertices and display seam pairing, test negative-height rejection and confirm posture, motion, skyline, guides and F9 are category-independent. See [verification.md](verification.md) for actual runs. These checks validate the declared model, not 100% real-world coastline accuracy.
