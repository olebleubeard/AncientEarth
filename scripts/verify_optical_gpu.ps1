param([int]$TimeoutSeconds = 600, [switch]$AtmosphereOnly, [switch]$SceneCost, [switch]$GeometryFixedPoint, [switch]$GeometryPeriodTwo, [switch]$Vulkan, [switch]$CachedField, [switch]$Dopri, [switch]$DopriDense, [switch]$DopriAuxiliary, [switch]$FaceNewton, [switch]$DopriHermite, [switch]$DopriCarried, [switch]$DopriHeight)
$ErrorActionPreference = 'Stop'
if (([int]$AtmosphereOnly.IsPresent + [int]$SceneCost.IsPresent + [int]$GeometryFixedPoint.IsPresent + [int]$GeometryPeriodTwo.IsPresent) -gt 1) { throw 'Choose one diagnostic mode.' }
$reportBase = if ($GeometryPeriodTwo) {'optical-geometry-period-two'} elseif ($GeometryFixedPoint) {'optical-geometry-fixed-point'} elseif ($SceneCost) {'optical-scene-cost'} elseif ($AtmosphereOnly) { 'optical-atmosphere-gpu-readback' } else { 'optical-gpu-readback' }
$probeArguments = if ($GeometryPeriodTwo) {'--geometry-period-two'} elseif ($GeometryFixedPoint) {'--geometry-fixed-point'} elseif ($SceneCost) {'--scene-cost'} elseif ($AtmosphereOnly) { '--atmosphere-only'} else { '--scene' }
if ($Vulkan) { $reportBase += '-vulkan'; $probeArguments += ' --vulkan' }
if ($CachedField) {$reportBase+='-cached-field';$probeArguments+=' --cached-field'}
if ($Dopri) {throw 'The first DP checkpoint is retired; use DopriDense for the current diagnostic. Historical generated WGSL and source hashes define old reports.'}
if ($DopriDense) {$reportBase+='-dopri-dense';$probeArguments+=' --dopri-dense'}
if ($DopriAuxiliary) {$reportBase+='-dopri-auxiliary';$probeArguments+=' --dopri-auxiliary'}
if ($DopriHeight) {$reportBase+='-dopri-height';$probeArguments+=' --dopri-height'}
if ($DopriCarried) {$reportBase+='-dopri-carried';$probeArguments+=' --dopri-carried'}
if ($DopriHermite) {$reportBase+='-dopri-hermite';$probeArguments+=' --dopri-hermite'}
if ($FaceNewton) {$reportBase+='-newton';$probeArguments+=' --face-newton'}
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
if ($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 1800) { throw 'Timeout must be 1..1800 seconds.' }
$libraries = @{}
foreach ($crate in @('earth_math','serde_json','wgpu','flate2')) {
    $library = Get-ChildItem -LiteralPath (Join-Path $workspace 'target/debug/deps') -Filter "lib$crate-*.rlib" |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (!$library) { throw "Build the desktop application first; missing $crate library." }
    $libraries[$crate] = $library.FullName
}
New-Item -ItemType Directory -Force -Path '.cache','artifacts' | Out-Null
& rustc --edition=2024 scripts/optical_gpu_probe.rs -L dependency=target/debug/deps `
    --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" `
    --extern "wgpu=$($libraries['wgpu'])" --extern "flate2=$($libraries['flate2'])" -o .cache/optical-gpu-probe.exe
if ($LASTEXITCODE -ne 0) { throw 'GPU diagnostic compilation failed.' }
$sources = @('scripts/optical_gpu_probe.rs','scripts/optical_shader_variants.rs','scripts/optical_marker_corner.wgsl','scripts/optical_face_newton.wgsl','scripts/optical_face_dense.wgsl','scripts/generate_optical_height_variant.py','src/desktop/shaders/refraction_dopri_height.wesl','src/desktop/shaders/refraction_dopri_height_trace.wesl','src/desktop/shaders/refraction_dopri_carried.wesl','src/desktop/shaders/refraction_dopri_carried_trace.wesl','src/desktop/shaders/refraction_dopri_hermite.wesl','src/desktop/shaders/refraction_dopri_hermite_trace.wesl','src/desktop/shaders/refraction_dopri.wesl','src/desktop/shaders/refraction_dopri_auxiliary.wesl','src/desktop/shaders/refraction_dopri_trace.wesl','src/desktop/shaders/refraction.wesl',
    'src/desktop/shaders/refraction_scene.wesl','crates/earth-math/src/atmosphere/gpu.rs',
    'crates/earth-math/src/atmosphere/gpu/scene.rs') | ForEach-Object {
    @{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()}
}
$libraryProvenance = @{}
foreach ($crate in $libraries.Keys) {
    $file = Get-Item -LiteralPath $libraries[$crate]
    $libraryProvenance[$crate] = @{path=$file.FullName; modified_utc=$file.LastWriteTimeUtc.ToString('o');
        sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
}
@{created_utc=[DateTime]::UtcNow.ToString('o'); sources=$sources; libraries=$libraryProvenance} |
    ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "artifacts/$reportBase-sources.json"
$process = Start-Process -FilePath (Join-Path $workspace '.cache/optical-gpu-probe.exe') `
    -ArgumentList $probeArguments -WorkingDirectory $workspace -WindowStyle Hidden -PassThru `
    -RedirectStandardOutput (Join-Path $workspace "artifacts/$reportBase.stdout.log") `
    -RedirectStandardError (Join-Path $workspace "artifacts/$reportBase.stderr.log")
$processHandle = $process.Handle # Retain the native handle so exit status remains available.
$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while (!$process.WaitForExit(1000)) {
    if ([DateTime]::UtcNow -gt $deadline) { $process.Kill(); throw 'GPU diagnostic exceeded the declared timeout.' }
}
$process.Refresh()
if ($process.ExitCode -ne 0) { throw "GPU diagnostic failed: exit $($process.ExitCode). See artifacts/$reportBase.stderr.log." }
Write-Output "Readback written to artifacts/$reportBase.json; inspect the recorded finite cases and review thresholds."
