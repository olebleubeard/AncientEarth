# CPU-only orchestration for the independently reviewed deviation cost-screen writer.
param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-deviation-stencil',
    [switch]$Flow,
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
    $crates = if ($Flow) { @('serde_json') } else { @('earth_math', 'serde_json') }
    $source = if ($Flow) { 'scripts/optical_deviation_flow.rs' } else { 'scripts/optical_deviation_stencil.rs' }
    $libraries = @{}
    foreach ($crate in $crates) {
        $item = Get-ChildItem -LiteralPath target/math-verification/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Missing isolated math dependency: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $fixtureInputs = if ($Flow) { @(
        $source, 'scripts/optical_taylor.rs', 'scripts/optical_second_variation.rs',
        'scripts/optical_deviation_bounds.rs',
        'artifacts/optical-deviation-stencil-progress-first.json'
    ) } else { @(
        $source, 'scripts/optical_refractive_deviation.rs', 'scripts/optical_shared_scalar.rs',
        'artifacts/optical-height-performance-conditioning.json',
        'artifacts/optical-shared-transport-cell4-first.json'
    ) }
    $inputs = @(
        'Cargo.toml', 'Cargo.lock',
        'scripts/verify_optical_deviation_stencil.ps1'
    ) + $fixtureInputs + @($libraries.Values)
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
    $executableName = if ($Flow) { '.cache/optical-deviation-flow.exe' } else { '.cache/optical-deviation-stencil.exe' }
    $executable = Join-Path $projectRoot $executableName
    $compilerArgs = @('--edition=2024', '-O', $source, '-L', 'dependency=target/math-verification/debug/deps')
    foreach ($crate in $crates) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    & rustc @compilerArgs
    if ($LASTEXITCODE -ne 0) { throw 'Deviation cost-screen compilation failed.' }
    if (($before | ConvertTo-Json -Compress) -ne (@(Get-InputFingerprints) | ConvertTo-Json -Compress)) {
        throw 'Inputs changed during compilation; do not execute this build.'
    }
    if ($CompileOnly) { Write-Output 'CPU deviation cost-screen preparer compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        inputs = $before
        executable = $executable
        executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        compiler_inputs_unchanged = $true
        compiler_arguments = $compilerArgs
        compiler_version = (& rustc --version)
        flow_bounds_screen = [bool]$Flow
        scope = 'CPU-only operation-cost and rejection screen. No certified admission, physical-ray acceptance or GPU performance claim.'
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
                throw 'CPU deviation cost-screen preparation exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "CPU deviation cost-screen preparation exited $($process.ExitCode); evidence retained." }
        foreach ($suffix in @('.json')) {
            if (!(Test-Path -LiteralPath "$prefix$suffix")) { throw "Missing cost-screen output: $suffix" }
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
