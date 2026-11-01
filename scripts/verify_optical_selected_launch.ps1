param(
    [int]$TimeoutSeconds=600,
    [ValidatePattern('^[a-z0-9][a-z0-9-]*$')][string]$Label,
    [string]$CandidateShader,
    [switch]$Strict
)
$ErrorActionPreference='Stop'
if($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 1800){throw 'Invalid timeout.'}
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
if($Strict -and $CandidateShader){throw 'Choose -Strict or -CandidateShader, not both.'}
if(($Strict -or $CandidateShader) -and !$Label){throw 'A new -Label is required for a modified diagnostic.'}
$outputPrefix='artifacts/optical-selected-launch-review'
if($Label){$outputPrefix+="-$Label"}
if(Get-ChildItem -LiteralPath (Split-Path $outputPrefix -Parent) -Filter "$(Split-Path $outputPrefix -Leaf)*" -ErrorAction SilentlyContinue){throw 'Output prefix already exists; choose a new label to preserve completed and failed evidence.'}
$processArguments=@('--output-prefix',('"{0}"' -f $outputPrefix))
if($Strict){$processArguments+='--strict'}
if($CandidateShader){
    $CandidateShader=(Resolve-Path -LiteralPath $CandidateShader).Path
    $processArguments+=@('--candidate-shader',('"{0}"' -f $CandidateShader))
}
$libraries=@{}
foreach($crate in @('earth_math','serde_json','wgpu','flate2')) {
    $library=Get-ChildItem -LiteralPath (Join-Path $workspace 'target/debug/deps') -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if(!$library){throw "Existing desktop dependency missing: $crate"}
    $libraries[$crate]=$library.FullName
}
& rustc --edition=2024 scripts/optical_selected_launch_review.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" --extern "flate2=$($libraries['flate2'])" -o .cache/optical-selected-launch-review.exe
if($LASTEXITCODE -ne 0){throw 'Selected-launch diagnostic compilation failed.'}
$sourcePaths=@('scripts/optical_selected_launch_review.rs','scripts/verify_optical_selected_launch.ps1','scripts/optical_gpu_probe.rs','scripts/optical_shader_variants.rs','scripts/optical_coverage_local.wgsl','artifacts/optical-selected-launch-input-checkpoint.json','artifacts/optical-selected-launch-input-candidate.wgsl','artifacts/optical-selected-launch-input-baseline.wgsl','.cache/topography-wide-area-qa/selection.json','assets/weather/gfs-regional-reference.grib2','assets/weather/hrrr-reference.grib2')
if($CandidateShader){$sourcePaths+=$CandidateShader}
$sources=$sourcePaths | ForEach-Object { @{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()} }
$provenance=@{}
foreach($crate in $libraries.Keys){$file=Get-Item -LiteralPath $libraries[$crate];$provenance[$crate]=@{path=$file.FullName;modified_utc=$file.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
@{created_utc=[DateTime]::UtcNow.ToString('o');sources=$sources;libraries=$provenance;strict=$Strict.IsPresent;candidate_shader=$CandidateShader}|ConvertTo-Json -Depth 5|Set-Content -LiteralPath "$outputPrefix-sources.json"
$process=Start-Process -FilePath (Join-Path $workspace '.cache/optical-selected-launch-review.exe') -ArgumentList $processArguments -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $workspace "$outputPrefix.stdout.log") -RedirectStandardError (Join-Path $workspace "$outputPrefix.stderr.log")
$processHandle=$process.Handle
$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while(!$process.WaitForExit(1000)){if([DateTime]::UtcNow -gt $deadline){$process.Kill();throw 'Selected-launch diagnostic timed out.'}}
$process.Refresh()
if($process.ExitCode -ne 0){throw "Selected-launch diagnostic exited $($process.ExitCode); inspect stderr log."}
@('candidate','baseline') | ForEach-Object { $path="$outputPrefix-$_.wgsl"; @{path=$path;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()} } | ConvertTo-Json | Set-Content -LiteralPath "$outputPrefix-shaders.json"
Write-Output "Report written to $outputPrefix.json"
