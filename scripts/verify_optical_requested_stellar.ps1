param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-requested-stellar-probe',
    [ValidateRange(1, 900)][int]$TimeoutSeconds = 300,
    [switch]$CompileOnly,
    [switch]$ConsumerCheck,
    [switch]$Compact,
    [switch]$ChainTiming
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue)) {
        throw 'Diagnostic evidence already exists; choose a new name.'
    }
    $libraries = @{}
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu', 'flate2')) {
        $library = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$library) { throw "Build desktop dependencies first: $crate" }
        $libraries[$crate] = $library.FullName
    }
    $sourcePaths = @(
        'scripts/optical_requested_stellar_probe.rs', 'scripts/optical_requested_stellar_fixture.rs',
        'scripts/optical_stellar_requested.wgsl', 'scripts/optical_stellar_pixels.wgsl',
        'scripts/verify_optical_requested_stellar.ps1',
        'scripts/optical_gpu_probe.rs', 'scripts/optical_shader_variants.rs',
        'src/desktop/shaders/refraction.wesl', 'src/desktop/shaders/refraction_scene.wesl',
        'src/desktop/shaders/stars_common.wesl', 'crates/earth-math/src/rendering/stars.rs',
        'crates/earth-math/src/atmosphere/gpu.rs', 'crates/earth-math/src/atmosphere/gpu/scene.rs',
        'assets/weather/gfs-regional-reference.grib2', 'assets/weather/hrrr-reference.grib2',
        '.cache/topography-wide-area-qa/selection.json',
        'artifacts/optical-selected-launch-input-baseline.wgsl',
        'artifacts/optical-stellar-table-probe.wgsl', 'artifacts/optical-stellar-table-probe.json'
    )
    $inputFiles = @($sourcePaths) + @($libraries.Values)
    function Get-InputFingerprints {
        foreach ($path in $inputFiles) {
            $item = Get-Item -LiteralPath $path
            [ordered]@{ path = [IO.Path]::GetRelativePath($projectRoot, $item.FullName); sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
    }
    $beforeCompilation = @(Get-InputFingerprints)
    $executable = Join-Path $projectRoot '.cache/optical-requested-stellar-probe.exe'
    & rustc --edition=2024 scripts/optical_requested_stellar_probe.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" --extern "flate2=$($libraries['flate2'])" -o $executable
    if ($LASTEXITCODE -ne 0) { throw 'Requested stellar diagnostic compilation failed.' }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Diagnostic inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{path=[IO.Path]::GetRelativePath($projectRoot, $executable);sha256=(Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()})
        compiler_inputs_unchanged = $true
        scope = 'Exact fixture/harness/source/reference/compiler-input fingerprints and diagnostic executable identity. Not native application frame-rate evidence.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $arguments = @('--atmosphere-only', '--name', $Name)
    if ($ConsumerCheck) { $arguments += '--consumer-check' }
    if ($Compact) { $arguments += '--compact' }
    if ($ChainTiming) { $arguments += '--consumer-check', '--chain-timing' }
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                throw 'Requested stellar diagnostic exceeded its watchdog.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Requested stellar diagnostic exited $($process.ExitCode); raw evidence retained." }
        if (!(Test-Path -LiteralPath "$prefix.json")) { throw 'Diagnostic report missing.' }
    } finally {
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $afterRun = @(Get-InputFingerprints)
        $inputsStable = ($beforeCompilation | ConvertTo-Json -Compress) -eq ($afterRun | ConvertTo-Json -Compress)
        [ordered]@{inputs_unchanged_through_run=$inputsStable;after_run=$afterRun} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-input-stability.json" -Encoding utf8
        $outputs = Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -File | Where-Object { $_.Name -ne "$Name-outputs.json" }
        @($outputs | ForEach-Object { [ordered]@{ path = [IO.Path]::GetRelativePath($projectRoot, $_.FullName); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() } }) | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-outputs.json" -Encoding utf8
    }
    if (!$inputsStable) { throw 'Diagnostic inputs changed while running; preserved evidence is not a frozen-input comparison.' }
    Write-Output "Wrote $prefix.json"
} finally {
    Pop-Location
}
