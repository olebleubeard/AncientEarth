param([int]$Width=1920,[int]$Height=1080,[int]$Group=64,[ValidateSet('full','air','cached','cached-layer','dopri-dense','dopri-dense-air','dopri-dense-scene-stub-air','dopri-auxiliary','dopri-auxiliary-newton','dopri-hermite','dopri-carried','dopri-height','dopri-hermite-corners','dopri-hermite-air','full-corners')][string]$Variant='full',[int]$TimeoutSeconds=600,[switch]$CompareFull,[string]$CompareVariant="full",[switch]$UpperSky)
$ErrorActionPreference='Stop'
if ($Width -lt 16 -or $Height -lt 16 -or [long]$Width*$Height -gt 2073600) { throw 'Diagnostic dimensions must contain 16x16 through 1920x1080 pixels.' }
if ($Group -notin @(32,64,128) -or $TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 1800) { throw 'Invalid group size or timeout.' }
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
$libraries=@{}
foreach($crate in @('earth_math','serde_json','wgpu','flate2')) {
    $library=Get-ChildItem -LiteralPath (Join-Path $workspace 'target/debug/deps') -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if(!$library){throw "Build the desktop dependencies first: $crate missing."}
    $libraries[$crate]=$library.FullName
}
& rustc --edition=2024 scripts/optical_trace_performance.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" --extern "flate2=$($libraries['flate2'])" -o .cache/optical-trace-performance.exe
if($LASTEXITCODE -ne 0){throw 'Diagnostic compilation failed.'}
$name="optical-trace-performance-${Width}x${Height}-g${Group}"
if($Variant -ne 'full'){$name+="-$Variant"}
$name+="-q16serial"
if($UpperSky){$name+="-upper-sky"}
if($CompareVariant -ne "full"){$name+="-vs-$CompareVariant"}
$sources=@('scripts/optical_trace_performance.rs','scripts/optical_gpu_probe.rs','scripts/optical_shader_variants.rs','scripts/optical_marker_corner.wgsl','scripts/optical_face_newton.wgsl','scripts/generate_optical_height_variant.py','src/desktop/shaders/refraction_dopri_height.wesl','src/desktop/shaders/refraction_dopri_height_trace.wesl','src/desktop/shaders/refraction_dopri_carried.wesl','src/desktop/shaders/refraction_dopri_carried_trace.wesl','src/desktop/shaders/refraction_dopri_hermite.wesl','src/desktop/shaders/refraction_dopri_hermite_trace.wesl','scripts/optical_face_dense.wgsl','scripts/optical_face_newton.wgsl','src/desktop/shaders/refraction_dopri.wesl','src/desktop/shaders/refraction_dopri_auxiliary.wesl','src/desktop/shaders/refraction_dopri_trace.wesl','src/desktop/shaders/refraction.wesl','src/desktop/shaders/refraction_scene.wesl','crates/earth-math/src/atmosphere/gpu.rs','crates/earth-math/src/atmosphere/gpu/scene.rs','.cache/topography-wide-area-qa/selection.json','assets/weather/gfs-regional-reference.grib2','assets/weather/hrrr-reference.grib2') | ForEach-Object { @{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()} }
$provenance=@{}
foreach($crate in $libraries.Keys){$file=Get-Item -LiteralPath $libraries[$crate];$provenance[$crate]=@{path=$file.FullName;modified_utc=$file.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
@{created_utc=[DateTime]::UtcNow.ToString('o');sources=$sources;libraries=$provenance}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath "artifacts/$name-sources.json"
$probeArguments=@('--width',$Width,'--height',$Height,'--group',$Group,'--variant',$Variant)
if($UpperSky){$probeArguments+='--upper-sky'}
if($CompareFull){$probeArguments+='--compare-full';$probeArguments+=@('--compare-variant',$CompareVariant)}
$process=Start-Process -FilePath (Join-Path $workspace '.cache/optical-trace-performance.exe') -ArgumentList $probeArguments -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $workspace "artifacts/$name.stdout.log") -RedirectStandardError (Join-Path $workspace "artifacts/$name.stderr.log")
$processHandle=$process.Handle
$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while(!$process.WaitForExit(1000)){if([DateTime]::UtcNow -gt $deadline){$process.Kill();throw 'GPU diagnostic timed out.'}}
$process.Refresh()
if($process.ExitCode -ne 0){throw "GPU diagnostic exited $($process.ExitCode); inspect artifacts/$name.stderr.log."}
@{path="artifacts/$name.wgsl";sha256=(Get-FileHash -LiteralPath "artifacts/$name.wgsl" -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json|Set-Content -LiteralPath "artifacts/$name-shader.json"
if($CompareFull){@{path="artifacts/$name-comparison-baseline.wgsl";sha256=(Get-FileHash -LiteralPath "artifacts/$name-comparison-baseline.wgsl" -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json|Set-Content -LiteralPath "artifacts/$name-comparison-baseline-shader.json"}
Write-Output "Report written to artifacts/$name.json"
