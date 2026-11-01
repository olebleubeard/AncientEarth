param([int]$TimeoutSeconds=180,[switch]$CompileOnly,[ValidateRange(0,2)][int]$Updates=2,[ValidateRange(1,64)][int]$Inner=1)
$ErrorActionPreference='Stop'
if($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 900){throw 'Timeout must be 1..900 seconds.'}
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
$prefix="artifacts/optical-scalar-query-u$Updates-i$Inner"
if(!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "optical-scalar-query-u$Updates-i$Inner*" -ErrorAction SilentlyContinue)){throw 'Preserve existing scalar query evidence before rerunning.'}
$libraries=@{}
foreach($crate in @('earth_math','serde_json','wgpu')){
    $item=Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if(!$item){throw "Rebuild current desktop dependencies first: $crate"}
    $libraries[$crate]=$item.FullName
}
& rustc --edition=2024 scripts/optical_scalar_query_probe.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" -o .cache/optical-scalar-query-probe.exe
if($LASTEXITCODE -ne 0){throw 'Scalar query compilation failed.'}
if($CompileOnly){Write-Output 'Scalar query probe compiled; no GPU execution.';return}
$paths=@('scripts/optical_scalar_query_probe.rs','scripts/optical_scalar_query.wgsl','scripts/verify_optical_scalar_query.ps1','scripts/optical_scalar_f32.rs','src/desktop/shaders/refraction.wesl','crates/earth-math/src/atmosphere/gpu.rs')
$sources=$paths|ForEach-Object {@{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()}}
$linked=@{}
foreach($crate in $libraries.Keys){$file=Get-Item -LiteralPath $libraries[$crate];$linked[$crate]=@{path=$file.FullName;modified_utc=$file.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
@{created_utc=[DateTime]::UtcNow.ToString('o');sources=$sources;libraries=$linked;executable_sha256=(Get-FileHash -LiteralPath .cache/optical-scalar-query-probe.exe -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath "$prefix-sources.json"
$process=Start-Process -FilePath (Join-Path $workspace '.cache/optical-scalar-query-probe.exe') -ArgumentList @('--updates',"$Updates",'--inner',"$Inner") -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $workspace "$prefix.stdout.log") -RedirectStandardError (Join-Path $workspace "$prefix.stderr.log")
$processHandle=$process.Handle
$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while(!$process.WaitForExit(1000)){if([DateTime]::UtcNow -gt $deadline){$process.Kill();throw 'Scalar GPU query exceeded bounded timeout.'}}
$process.Refresh()
if($process.ExitCode -ne 0){throw "Scalar GPU query exited $($process.ExitCode); logs retained."}
@{path="$prefix.wgsl";sha256=(Get-FileHash -LiteralPath "$prefix.wgsl" -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json|Set-Content -LiteralPath "$prefix-shader.json"
Write-Output "Wrote $prefix.json"
