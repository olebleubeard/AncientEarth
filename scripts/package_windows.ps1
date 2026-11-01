param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [switch]$SideBySide
)
$ErrorActionPreference = "Stop"
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Push-Location -LiteralPath $projectRoot
try {
    python scripts/check_asset_licenses.py
    if ($LASTEXITCODE -ne 0) { throw "Asset license audit failed" }
    python scripts/software_licenses.py --check
    if ($LASTEXITCODE -ne 0) { throw "Software license audit failed" }
    $builtExecutable = "target/$Profile/ancient-earth.exe"
    $buildArgs = @("build", "--locked", "--offline")
    if ($SideBySide) {
        # Windows locks a running executable. Build a separate application so
        # packaging does not close the user's current session.
        $builtExecutable = "target/$Profile/ancient-earth-preview.exe"
        $buildArgs = @("rustc", "--bin", "ancient-earth", "--locked", "--offline")
    }
    if ($Profile -eq "release") { $buildArgs += "--release" }
    if ($SideBySide) { $buildArgs += @("--", "-o", (Join-Path $projectRoot $builtExecutable)) }
    cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
    $name = "ancient-earth-windows-x64-$Profile-$stamp"
    $destination = Join-Path $projectRoot "artifacts/packages/$name"
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item -LiteralPath $builtExecutable -Destination (Join-Path $destination "ancient-earth.exe")
    foreach ($file in @("LICENSE", "NOTICE.md", "THIRD_PARTY_LICENSES.txt", "README.md", "Cargo.lock")) {
        Copy-Item -LiteralPath $file -Destination $destination
    }
    Copy-Item -LiteralPath "licenses" -Destination $destination -Recurse
    Copy-Item -LiteralPath "docs" -Destination $destination -Recurse
    $provenanceDirectory = Join-Path $destination "asset-provenance"
    New-Item -ItemType Directory -Path $provenanceDirectory | Out-Null
    foreach ($file in Get-ChildItem -LiteralPath "assets" -Recurse -File -Filter "*.provenance.json") {
        Copy-Item -LiteralPath $file.FullName -Destination $provenanceDirectory
    }
    # The two lunar maps share one record named provenance.json, which the
    # historical *.provenance.json glob does not select.
    Copy-Item -LiteralPath "assets/lunar/provenance.json" -Destination (Join-Path $provenanceDirectory "lunar.provenance.json")
    @"
Ancient Earth

Run ancient-earth.exe to start at 29.305549, -94.767015 on the ground, facing east.
Preparing graphics shows startup progress while shaders compile in the background.
Flight becomes available when the first scene is ready; Close app remains available.
Use --overview for the globe overview or --coast for a boundary inspection view.
Land and water default to the smooth WGS84 ellipsoid.
Topography Download estimates size and downloads/activates Copernicus elevations and water masks.
The default and maximum download radius is 100 miles (160.9344 km).
Elevations off restores smooth Earth and keeps the cache.
Load last saved area restores the completed terrain offline after restarting.
Source and retained mesh resolution are finite and are reported in the panel.
Water masks provide boundaries and categories only; imagery remains removed.
Natural Earth provides coarse global land/ocean context outside detailed mask coverage.
Old caches can add water masks with Download + enable while reusing elevation files.
No tides are simulated.
City catalogs, map pins and named location shortcuts have been removed. GPS entry remains.
Hold C to crouch. Press L to toggle the one-pixel skyline guide.
Turning L on also enables V. V hides/restores the ruler; move it with Left/Right or drag.
Imperial is the default; heights keep feet/metres with miles/km in parentheses.
Press 1 for marker settings, then confirm Spawn markers. Defaults: half-mile spacing,
40-mile reach, 100-foot left offset; 10 x 10 x 100-foot red/green banded prisms.
The first marker is beside you at the left offset; following markers extend ahead.
Upright black signs above the markers face you and are numbered 0001 onward.
Aim at a visible marker for its modeled hidden centerline height.
Press M for tape; aim the center dot and left-click A, then B. M hides/restores it.
The tape is a thin on-screen line with small distance labels, also visible in F2.
Terrain and markers support weather refraction. Tape endpoints follow curved sight lines;
tape distance labels still distinguish straight separation and WGS84 surface distance.
Changing the physical ground surface clears the old survey points and markers.
F9 toggles following Earth rotation. Play or step time to see Earth turn underneath.
Press F5 to switch between the 1080p window and native-monitor fullscreen.
Press F2 to hide or restore panels and the dot. L/V remain usable in clean view.
Use the left, right and bottom edge arrows to collapse or restore each panel.
All left navigation sections start collapsed. Click a heading to expand it.
Sections retain their expanded state during the session and reset on restart.
The first three top icons are Earth, Moon and Sun: click to aim; double-click to track.
Double-click the tracked icon again to stop; manual look or unlocked roll also stops tracking.
The fourth icon toggles horizon lock, also in Observer & Display. It starts enabled.
Horizon lock prevents sideways tilt near Earth; looking up/down remains free.
It releases automatically in space. --no-horizon-lock starts with free roll everywhere.
Observer & Display -> Show tooltips controls hover help.
Orbits & Axis Guides toggles the Earth and Moon axis/tilt diagrams.
NASA lunar maps provide real features and relief shading; geometry remains spherical.
Press I for Earth, Sun and Moon information; it also works in F2 clean view.
Left-click the scene to capture mouse look; Escape releases the cursor and stops flight.
WASD moves; Q/E rolls when horizon lock is inactive. Space/Shift moves body-up/down.
G lands you upright and level, preserving your heading. Shift no longer boosts speed.
Fly continuously from ground to space; orbital guides appear automatically.
Automatic flight speed adapts to nearby surfaces. Wheel sets base speed; Ctrl slows.
Aim at Earth turns your current view. O and Fit whole orbit are optional position presets.
Press R to exit telescope and reset the normal view without moving.
Eye Height and Datum Reference show whole feet and inches in Imperial mode.
Clean view keeps Height: in the upper right, measuring eyes above the active surface.
Sun, Moon & Time provides calendar entry, play/pause, speed and signed time steps.
Eclipse geometry and shadows use your coordinates and selected date.
Observer & Display provides eye-height settings and a persistent crouch toggle.
Its Frame limit defaults to 60 FPS; choose 30, 60, 120 or Unlimited.
Background windows are limited to 15 FPS and minimized windows to 5 FPS.
Its Show grass checkbox lets you inspect the smooth surface underneath.
EGM2008 supplies datum readouts and the optional DSM height conversion; not live tides.
Weather & Refraction provides an enable/disable toggle (off by default).
Custom weather below Refresh weather opens an editor seeded from the loaded column.
Edit layers and Apply custom weather; Use live weather restores automatic updates.
Custom settings last for this session and survive toggling refraction off and on.
Full-resolution curved-ray viewing is expensive; see the measured limits in docs.
Use Vulkan for refraction. DirectX 12 keeps view and measurements geometric after
native device-loss failures; the panel identifies this and retains weather data.
Use --weather for live NOAA forecasts at your in-app location and real UTC.
Use --weather-reference for the explicitly labeled fixed verification snapshot.
Use --weather-regional-reference for the fixed HRRR/GFS verification pair.
Refraction mode uses simplified lighting and hides decorative grass and its shadows.
Above 20 km eye height, view and measurements explicitly return to geometric mode.
Enabled terrain and markers remain compatible with eligible curved-ray viewing.
Stars occupy single physical pixels with a declared reference-turbulence model.
Weather profiles have finite resolution and do not guarantee actual mirage conditions.
Use --licenses to print embedded notices without starting graphics.
The GUI also has a Licenses & attribution window.

In PowerShell, to use weather refraction:
  `$env:WGPU_BACKEND = "vulkan"
  .\ancient-earth.exe --weather

If Vulkan startup fails, DirectX 12 supports geometric viewing:
  `$env:WGPU_BACKEND = "dx12"
  .\ancient-earth.exe --coast

Base scene data are embedded. Optional live weather requires NOAA network access.
System prerequisite: x64 Microsoft Visual C++ v14 runtime (not bundled).
If VCRUNTIME140.dll or VCRUNTIME140_1.dll is missing, install the x64 runtime:
https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist
Keep LICENSE, NOTICE.md, THIRD_PARTY_LICENSES.txt and licenses/ with redistributed copies.
See docs/user-guide.md and docs/development/licensing.md.
"@ | Set-Content -LiteralPath (Join-Path $destination "START-HERE.txt") -Encoding utf8
    $archive = "$destination.zip"
    Compress-Archive -LiteralPath $destination -DestinationPath $archive
    $checksum = Get-FileHash -LiteralPath $archive -Algorithm SHA256
    "$($checksum.Hash.ToLower())  $name.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Output "Packaged: $archive"
}
finally {
    Pop-Location
}
