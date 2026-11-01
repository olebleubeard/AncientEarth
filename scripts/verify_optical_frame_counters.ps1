param([ValidateSet('hermite','height')][string]$Variant='hermite',[int]$TimeoutSeconds=300,[switch]$CompileOnly)
$ErrorActionPreference='Stop'
if($TimeoutSeconds -lt 1 -or $TimeoutSeconds -gt 900){throw 'Invalid bounded timeout.'}
$workspace=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $workspace
$shader=if($Variant -eq 'hermite'){'artifacts/optical-trace-performance-1920x1080-g32-dopri-hermite-corners-q16serial.wgsl'}else{'artifacts/optical-trace-performance-1920x1080-g32-dopri-height-q16serial.wgsl'}
$prefix="artifacts/optical-frame-counters-$Variant"
if(!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "optical-frame-counters-$Variant*" -ErrorAction SilentlyContinue)){throw 'Counter evidence already exists; preserve it first.'}
$libraries=@{}
foreach($crate in @('serde_json','wgpu')){$item=Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib"|Sort-Object LastWriteTime -Descending|Select-Object -First 1;if(!$item){throw "Missing current dependency:$crate"};$libraries[$crate]=$item.FullName}
& rustc --edition=2024 scripts/optical_frame_counters.rs -L dependency=target/debug/deps --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" -o .cache/optical-frame-counters.exe
if($LASTEXITCODE -ne 0){throw 'Counter diagnostic compilation failed.'}
if($CompileOnly){Write-Output 'Compiled counters; no GPU execution.';return}
$paths=@('scripts/optical_frame_counters.rs','scripts/verify_optical_frame_counters.ps1',$shader,'artifacts/optical-requested-stellar-sparse-chain-uniform.bin','artifacts/optical-requested-stellar-sparse-chain-view.bin','artifacts/optical-requested-stellar-sparse-chain-geometry.bin','artifacts/optical-requested-stellar-sparse-chain-frames.bin')
$sources=$paths|ForEach-Object {@{path=$_;sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()}}
$linked=@{};foreach($crate in $libraries.Keys){$file=Get-Item -LiteralPath $libraries[$crate];$linked[$crate]=@{path=$file.FullName;modified_utc=$file.LastWriteTimeUtc.ToString('o');sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
@{created_utc=[DateTime]::UtcNow.ToString('o');sources=$sources;libraries=$linked;executable_sha256=(Get-FileHash -LiteralPath .cache/optical-frame-counters.exe -Algorithm SHA256).Hash.ToLowerInvariant()}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath "$prefix-sources.json"
$process=Start-Process -FilePath (Join-Path $workspace '.cache/optical-frame-counters.exe') -ArgumentList @($shader,$Variant) -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $workspace "$prefix.stdout.log") -RedirectStandardError (Join-Path $workspace "$prefix.stderr.log")
$processHandle=$process.Handle;$deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
while(!$process.WaitForExit(1000)){if([DateTime]::UtcNow -gt $deadline){$process.Kill();throw 'Bounded counter probe timed out.'}}
$process.Refresh();if($process.ExitCode -ne 0){throw "Counter probe exited $($process.ExitCode); logs retained."}
$outputs=Get-ChildItem -LiteralPath artifacts -Filter "optical-frame-counters-$Variant*"|Where-Object {$_.Extension -in @('.bin','.wgsl') }|ForEach-Object {@{path=$_.FullName;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}}
$outputs|ConvertTo-Json -Depth 4|Set-Content -LiteralPath "$prefix-outputs.json"
Write-Output "Wrote $prefix.json"
