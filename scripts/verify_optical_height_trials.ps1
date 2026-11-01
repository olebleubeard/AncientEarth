param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-height-trial-cost',
    [switch]$CompileOnly,
    [switch]$ValidateOnly,
    [switch]$FastBounds,
    [switch]$CartesianPromoted,
    [switch]$CartesianDirect,
    [ValidateRange(1, 900)][int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
if ($CompileOnly -and $ValidateOnly) { throw 'Choose compilation or offline validation.' }
if (($FastBounds -and $CartesianPromoted) -or ($CartesianDirect -and ($FastBounds -or $CartesianPromoted))) { throw 'Choose one trial fixture.' }
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue)) {
        throw 'Diagnostic evidence already exists; choose a new name.'
    }
    $libraries = @{}
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu')) {
        $item = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Rebuild current desktop dependencies first: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $sourcePaths = @(
        'scripts/optical_height_trial_probe.rs', 'scripts/optical_height_trial_fixture.rs', 'scripts/optical_gpu_runtime.rs',
        'scripts/optical_normal_trace_fixture.rs',
        'scripts/optical_normal_step.wgsl', 'scripts/optical_normal_interval.wgsl',
        'scripts/optical_normal_residual_fast.wgsl',
        'scripts/optical_normal_interval_fast.wgsl',
        'scripts/optical_height_fixture.rs', 'scripts/optical_height_step.wgsl',
        'scripts/optical_height_trace.wgsl', 'scripts/generate_optical_height_step.py',
        'scripts/optical_height_trial_capture.rs', 'scripts/optical_height_trial_split.rs',
        'scripts/optical_height_rhs_fixture.rs', 'scripts/optical_height_rhs_projected.wgsl',
        'scripts/optical_height_metric_fast.wgsl', 'scripts/optical_height_bounds_fast.wgsl',
        'scripts/optical_normal_trace.wgsl', 'scripts/verify_optical_height_trials.ps1',
        'artifacts/optical-normal-coordinate-current.json',
        'artifacts/optical-height-trial-capture.json',
        'artifacts/optical-height-trial-capture-readback.bin',
        'artifacts/optical-height-trial-capture-uniform.bin'
    )
    # Both fixture modules are compiled into the host. Retain the reviewed new
    # module and its transitive source/data inventory for every mode.
    $inventories = @('scripts/optical_cartesian_trial_inputs.txt', 'scripts/optical_cartesian_direct_inputs.txt')
    $fixtureInputs = @()
    foreach ($inventory in $inventories) {
        $listed = @(Get-Content -LiteralPath $inventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
        if (!$listed) { throw "Cartesian source and data inventory is empty: $inventory" }
        $fixtureInputs += $listed
    }
    $inputFiles = @($sourcePaths) + @($libraries.Values) + @('Cargo.toml', 'Cargo.lock') + $inventories + $fixtureInputs
    $inputFiles = @($inputFiles | Sort-Object -Unique)
    function Get-InputFingerprints {
        foreach ($path in $inputFiles) {
            $item = Get-Item -LiteralPath $path
            [ordered]@{
                path = [IO.Path]::GetRelativePath($projectRoot, $item.FullName)
                sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }
    }
    $beforeCompilation = @(Get-InputFingerprints)
    $executable = Join-Path $projectRoot '.cache/optical-height-trial-cost-probe.exe'
    $compilerArgs = @('--edition=2024', 'scripts/optical_height_trial_probe.rs', '-L', 'dependency=target/debug/deps')
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu')) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    if ($CompileOnly) {
        & rustc @compilerArgs
    } else {
        [ordered]@{
            created_utc = [DateTime]::UtcNow.ToString('o')
            inputs = $beforeCompilation
            compiler_arguments = $compilerArgs
            compiler_version = (& rustc --version)
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-compile-inputs.json" -Encoding utf8
        & rustc @compilerArgs *> "$prefix-compiler.log"
    }
    if ($LASTEXITCODE -ne 0) {
        if (!$CompileOnly) { Get-Content -LiteralPath "$prefix-compiler.log" -Tail 50 }
        throw 'Trial cost compilation failed; compiler evidence retained.'
    }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Diagnostic inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'Height trial cost probe compiled; no GPU execution.'; return }
    $compiledExecutableHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    $retainedExecutable = Join-Path $projectRoot "$prefix-executable.exe"
    Copy-Item -LiteralPath $executable -Destination $retainedExecutable
    if ((Get-FileHash -LiteralPath $retainedExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -ne $compiledExecutableHash) {
        throw 'Retained executable differs from the compiled binary.'
    }
    $executable = $retainedExecutable
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        executable_retained = $true
        fast_bounds = [bool]$FastBounds
        cartesian_promoted = [bool]$CartesianPromoted
        cartesian_direct = [bool]$CartesianDirect
        validation_only = [bool]$ValidateOnly
        scope = 'Frozen identical retained trial inputs; isolated nonadditive component timings, not full-frame or native application performance.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $probeArguments = @('--name', $Name)
    if ($FastBounds) { $probeArguments += '--fast-bounds' }
    if ($CartesianPromoted) { $probeArguments += '--cartesian-promoted' }
    if ($CartesianDirect) { $probeArguments += '--cartesian-direct' }
    if ($ValidateOnly) { $probeArguments += '--validate-only' }
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList $probeArguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    $timedOut = $false
    $failure = $null
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                $timedOut = $true
                throw 'Height trial cost exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Height trial cost exited $($process.ExitCode); raw evidence retained." }
        $expectedReport = if ($ValidateOnly) { "$prefix-validation.json" } else { "$prefix.json" }
        if (!(Test-Path -LiteralPath $expectedReport)) { throw 'Trial report missing.' }
    } catch {
        $failure = $_.Exception.Message
        throw
    } finally {
        $terminatedByWrapper = !$process.HasExited
        if ($terminatedByWrapper) { $process.Kill(); $process.WaitForExit() }
        $process.Refresh()
        $executableStable = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant() -eq $compiledExecutableHash
        [ordered]@{
            owned_process_id = $process.Id
            executable = $executable
            executable_sha256 = $compiledExecutableHash
            executable_retained = $true
            executable_unchanged_through_run = $executableStable
            arguments = $probeArguments
            started_utc = $started.ToString('o')
            finished_utc = [DateTime]::UtcNow.ToString('o')
            timeout_seconds = $TimeoutSeconds
            timed_out = $timedOut
            terminated_by_wrapper = $terminatedByWrapper
            exit_code = $process.ExitCode
            failure = $failure
        } | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-lifecycle.json" -Encoding utf8
        $afterRun = @(Get-InputFingerprints)
        $inputsStable = ($beforeCompilation | ConvertTo-Json -Compress) -eq ($afterRun | ConvertTo-Json -Compress)
        [ordered]@{
            inputs_unchanged_through_run = $inputsStable
            after_run = $afterRun
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-input-stability.json" -Encoding utf8
        $outputs = Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -File | Where-Object { $_.Name -ne "$Name-outputs.json" }
        @($outputs | ForEach-Object {
            [ordered]@{
                path = [IO.Path]::GetRelativePath($projectRoot, $_.FullName)
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }) | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-outputs.json" -Encoding utf8
    }
    if (!$inputsStable) { throw 'Diagnostic inputs changed while running; preserved evidence is not a frozen-input comparison.' }
    if (!$executableStable) { throw 'Retained executable changed during execution.' }
    Write-Output "Wrote $expectedReport"
} finally {
    Pop-Location
}
