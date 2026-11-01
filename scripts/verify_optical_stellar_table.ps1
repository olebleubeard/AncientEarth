param([int]$TimeoutSeconds=300)
$ErrorActionPreference='Stop'
if($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 900){throw 'Timeout must be 1..900 seconds.'}
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
$prefix='artifacts/optical-stellar-table-probe'
if(Get-ChildItem -LiteralPath artifacts -Filter 'optical-stellar-table-probe*' -ErrorAction SilentlyContinue){throw 'Preserve the existing stellar probe evidence before rerunning.'}
$libraries=@{}
foreach($crate in @('earth_math','serde_json','wgpu','flate2')){
    $item=Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if(!$item){throw "Rebuild current desktop dependencies first: $crate"}
    $libraries[$crate]=$item.FullName
}
& rustc --edition=2024 scripts/optical_stellar_table_probe.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" --extern "flate2=$($libraries['flate2'])" -o .cache/optical-stellar-table-probe.exe
if($LASTEXITCODE -ne 0){throw 'Stellar diagnostic compilation failed.'}
$paths=@('scripts/optical_stellar_table_probe.rs','scripts/verify_optical_stellar_table.ps1','scripts/optical_gpu_probe.rs','scripts/optical_shader_variants.rs','src/desktop/shaders/refraction.wesl','crates/earth-math/src/rendering/stars.rs','crates/earth-math/src/atmosphere/gpu.rs','assets/weather/gfs-regional-reference.grib2','assets/weather/hrrr-reference.grib2')
$sources=$paths|ForEach-Object {@{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()}}
$linked=@{}
foreach($crate in $libraries.Keys){$file=Get-Item -LiteralPath $libraries[$crate];$linked[$crate]=@{path=$file.FullName;modified_utc=$file.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
@{created_utc=[DateTime]::UtcNow.ToString('o');sources=$sources;libraries=$linked}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath "$prefix-sources.json"
$process=Start-Process -FilePath (Join-Path $workspace '.cache/optical-stellar-table-probe.exe') -ArgumentList '--atmosphere-only' -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $workspace "$prefix.stdout.log") -RedirectStandardError (Join-Path $workspace "$prefix.stderr.log")
$processHandle=$process.Handle
$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while(!$process.WaitForExit(1000)){if([DateTime]::UtcNow -gt $deadline){$process.Kill();throw 'Stellar GPU diagnostic exceeded its bounded timeout.'}}
$process.Refresh()
if($process.ExitCode -ne 0){throw "Stellar GPU diagnostic exited $($process.ExitCode); retained logs contain the failure."}
@{path="$prefix.wgsl";sha256=(Get-FileHash -LiteralPath "$prefix.wgsl" -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json|Set-Content -LiteralPath "$prefix-shader.json"
Write-Output "Wrote $prefix.json"
