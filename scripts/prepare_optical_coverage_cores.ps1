# CPU-only orchestration for the independently reviewed coverage metadata writer.
param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-canonical-coverage-cores',
    [switch]$CompileOnly,
    [ValidateRange(1, 600)][int]$TimeoutSeconds = 180
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue)) {
        throw 'Evidence already exists; choose a new name.'
    }
    $libraries = @{}
    foreach ($crate in @('earth_math', 'serde_json', 'flate2', 'wgpu')) {
        $item = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Missing current desktop dependency: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $inputs = @(
        'Cargo.toml', 'Cargo.lock',
        'scripts/prepare_optical_coverage_cores.ps1', 'scripts/prepare_optical_coverage_cores.rs',
        'scripts/optical_canonical_coverage_packet.rs', 'scripts/optical_boundary_coverage.rs',
        'scripts/optical_normal_ideal_frame.rs',
        'scripts/optical_taylor.rs', 'scripts/optical_gpu_probe.rs', 'scripts/optical_shader_variants.rs',
        'artifacts/optical-normal-scene-packet.json', 'artifacts/optical-normal-scene-packet-certificates.bin',
        'artifacts/optical-requested-stellar-sparse-chain-geometry.bin',
        '.cache/topography-wide-area-qa/selection.json',
        'assets/geoid/egm2008-native.aeg.gz',
        'assets/boundaries/region-01.aeb', 'assets/boundaries/region-02.aeb',
        'assets/boundaries/region-03.aeb', 'assets/boundaries/region-04.aeb',
        'assets/weather/gfs-regional-reference.grib2', 'assets/weather/hrrr-reference.grib2'
    ) + @(Get-ChildItem -LiteralPath '.cache/topography-wide-area-qa/assets' -Filter '*.tif' -File | ForEach-Object FullName) + @($libraries.Values)
    $inputs = @($inputs | Sort-Object -Unique)
    function Get-InputFingerprints {
        foreach ($path in $inputs) {
            $item = Get-Item -LiteralPath $path
            [ordered]@{
                path = [IO.Path]::GetRelativePath($projectRoot, $item.FullName)
                sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }
    }
    $before = @(Get-InputFingerprints)
    $executable = Join-Path $projectRoot '.cache/prepare-optical-coverage-cores.exe'
    $compilerArgs = @('--edition=2024', 'scripts/prepare_optical_coverage_cores.rs', '-L', 'dependency=target/debug/deps')
    foreach ($crate in @('earth_math', 'serde_json', 'flate2', 'wgpu')) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    & rustc @compilerArgs
    if ($LASTEXITCODE -ne 0) { throw 'Coverage preparer compilation failed.' }
    if (($before | ConvertTo-Json -Compress) -ne (@(Get-InputFingerprints) | ConvertTo-Json -Compress)) {
        throw 'Inputs changed during compilation; do not execute this build.'
    }
    if ($CompileOnly) { Write-Output 'CPU coverage preparer compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        inputs = $before
        executable = $executable
        executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        compiler_inputs_unchanged = $true
        scope = 'CPU-only immutable coverage metadata preparation; no ray, contact or GPU performance acceptance.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -ArgumentList @($prefix) -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    $timedOut = $false
    $failure = $null
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                $timedOut = $true
                throw 'CPU coverage preparation exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "CPU coverage preparation exited $($process.ExitCode); evidence retained." }
        foreach ($suffix in @('.json', '-cores.bin')) {
            if (!(Test-Path -LiteralPath "$prefix$suffix")) { throw "Missing coverage output: $suffix" }
        }
    } catch {
        $failure = $_.Exception.Message
        throw
    } finally {
        $terminated = !$process.HasExited
        if ($terminated) { $process.Kill(); $process.WaitForExit() }
        $process.Refresh()
        [ordered]@{
            owned_process_id = $process.Id
            started_utc = $started.ToString('o')
            finished_utc = [DateTime]::UtcNow.ToString('o')
            timeout_seconds = $TimeoutSeconds
            timed_out = $timedOut
            terminated_by_wrapper = $terminated
            exit_code = $process.ExitCode
            failure = $failure
        } | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-lifecycle.json" -Encoding utf8
        $after = @(Get-InputFingerprints)
        $stable = ($before | ConvertTo-Json -Compress) -eq ($after | ConvertTo-Json -Compress)
        [ordered]@{
            inputs_unchanged_through_run = $stable
            after_run = $after
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-input-stability.json" -Encoding utf8
        @(Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -File | Where-Object { $_.Name -ne "$Name-outputs.json" } | ForEach-Object {
            [ordered]@{
                path = [IO.Path]::GetRelativePath($projectRoot, $_.FullName)
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }) | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-outputs.json" -Encoding utf8
    }
    if (!$stable) { throw 'Inputs changed during preparation; evidence is not frozen.' }
    Write-Output "Prepared $prefix; independent packet review required."
} finally {
    Pop-Location
}
