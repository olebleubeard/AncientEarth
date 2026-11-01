# Mechanical host for the independently reviewed continuation experiment.
param(
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Name,
    [switch]$CompileOnly,
    [switch]$ValidateOnly,
    [switch]$Full,
    [switch]$Time,
    [switch]$Reference,
    [switch]$ReduceBookkeeping,
    [switch]$OriginalClearance,
    [switch]$CartesianSuffix,
    [switch]$CartesianSuffixTrace,
    [switch]$CartesianSuffixTight,
    [ValidatePattern('^[a-z0-9-]+$')][string]$CartesianSuffixBranch,
    [ValidateSet('selected', 'atmosphere-100-gradient')][string]$CartesianSuffixF64,
    [ValidateSet('readiness', 'fisheye', 'vacuum-outward', 'vacuum-inward', 'atmosphere-minus20-vacuum', 'atmosphere-minus20-gradient', 'atmosphere-100-vacuum', 'atmosphere-100-gradient', 'atmosphere-5000-vacuum', 'atmosphere-5000-gradient', 'returning-gradient')][string]$CartesianCompositeF64,
    [switch]$CartesianCompositeReference,
    [switch]$CartesianCompositeFull,
    [switch]$BranchReference,
    [ValidateSet(1, 8, 32, 128)][int]$SliceAttempts = 32,
    [ValidateRange(1, 900)][int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
if ($CartesianCompositeFull -and (!$Full -or $Reference -or $ReduceBookkeeping -or $OriginalClearance -or $CartesianSuffix -or $CartesianSuffixTrace -or $CartesianSuffixTight -or $CartesianSuffixBranch -or $CartesianSuffixF64 -or $CartesianCompositeF64 -or $CartesianCompositeReference -or $BranchReference -or $PSBoundParameters.ContainsKey('SliceAttempts'))) { throw '-CartesianCompositeFull requires -Full and excludes other fixtures.' }
if ($BranchReference -and !$CartesianSuffixBranch) { throw '-BranchReference requires -CartesianSuffixBranch.' }
if ($CartesianSuffixBranch) { $CartesianSuffixTight = $true }
if ($CartesianSuffixTight) { $CartesianSuffixTrace = $true }
if ($CartesianSuffixTrace -and ($Full -or $Time)) { throw '-CartesianSuffixTrace is an untimed selected-record diagnostic.' }
if ($CartesianSuffixTrace) { $CartesianSuffix = $true }
if ($CartesianSuffixF64 -and ($Full -or $Time -or $Reference -or $ReduceBookkeeping -or $OriginalClearance -or $CartesianSuffix -or $CartesianCompositeF64 -or $BranchReference -or $PSBoundParameters.ContainsKey('SliceAttempts'))) { throw '-CartesianSuffixF64 permits only the two selected, untimed cases.' }
if ($CartesianCompositeReference -and $CartesianCompositeF64 -ne 'returning-gradient') { throw '-CartesianCompositeReference requires -CartesianCompositeF64 returning-gradient.' }
if ($CartesianCompositeF64 -and ($Full -or $Time -or $Reference -or $ReduceBookkeeping -or $OriginalClearance -or $CartesianSuffix -or $CartesianSuffixF64 -or $BranchReference -or $PSBoundParameters.ContainsKey('SliceAttempts'))) { throw '-CartesianCompositeF64 is a separate selected, untimed correctness mode.' }
if ($OriginalClearance -and ($CartesianSuffix -or $Reference -or $ReduceBookkeeping -or $PSBoundParameters.ContainsKey('SliceAttempts'))) { throw '-OriginalClearance is a separate original-solver fixture.' }
if ($Reference -and $ReduceBookkeeping) { throw '-ReduceBookkeeping is a continuation variant.' }
if ($CartesianSuffix -and ($Reference -or $ReduceBookkeeping -or $PSBoundParameters.ContainsKey('SliceAttempts'))) { throw '-CartesianSuffix is separate from reference and sliced continuation modes.' }
if ($Time -and !$Full) { throw '-Time requires -Full.' }
if ($CompileOnly -and $ValidateOnly) { throw 'Choose compilation or offline validation.' }
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue)) {
        throw 'Evidence already exists; choose a new name.'
    }
    $libraries = @{}
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu', 'flate2', 'bevy')) {
        $item = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Rebuild current desktop dependencies first: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $nativeInputs = @()
    $nativeDirectories = @(Get-ChildItem -LiteralPath target/debug/build -Filter 'windows_x86_64_msvc-*' -Directory | ForEach-Object {
        $buildOutput = Join-Path $_.FullName 'output'
        if (Test-Path -LiteralPath $buildOutput) {
            foreach ($line in Get-Content -LiteralPath $buildOutput) {
                if ($line.StartsWith('cargo:rustc-link-search=native=')) {
                    $directory = $line.Substring('cargo:rustc-link-search=native='.Length)
                    $nativeInputs += $buildOutput
                    $nativeInputs += @(Get-ChildItem -LiteralPath $directory -Filter '*.lib' -File | ForEach-Object FullName)
                    $directory
                }
            }
        }
    } | Sort-Object -Unique)
    if (!$nativeDirectories) { throw 'Windows native dependency build output is missing.' }
    # Reviewed source/runtime dependency inventory. It must exist before this
    # host can run; the fixture's data blobs are included, not just its source.
    $inventory = 'scripts/optical_normal_scene_inputs.txt'
    $fixtureInputs = @(Get-Content -LiteralPath $inventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$fixtureInputs) { throw 'Scene fixture dependency inventory is empty.' }
    $suffixInventory = 'scripts/optical_cartesian_suffix_inputs.txt'
    $suffixInputs = @(Get-Content -LiteralPath $suffixInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$suffixInputs) { throw 'Cartesian suffix source and runtime inventory is empty.' }
    $traceInventory = 'scripts/optical_cartesian_suffix_trace_inputs.txt'
    $traceInputs = @(Get-Content -LiteralPath $traceInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$traceInputs) { throw 'Cartesian suffix trace inventory is empty.' }
    $tightInventory = 'scripts/optical_cartesian_suffix_tight_inputs.txt'
    $tightInputs = @(Get-Content -LiteralPath $tightInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$tightInputs) { throw 'Cartesian suffix convergence inventory is empty.' }
    $branchInventory = 'scripts/optical_cartesian_suffix_branch_inputs.txt'
    $branchInputs = @(Get-Content -LiteralPath $branchInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$branchInputs) { throw 'Cartesian suffix branch inventory is empty.' }
    $atmosphereInventory = 'scripts/optical_cartesian_suffix_atmosphere_inputs.txt'
    $atmosphereInputs = @(Get-Content -LiteralPath $atmosphereInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$atmosphereInputs) { throw 'Cartesian suffix atmosphere inventory is empty.' }
    $clearanceInventory = 'scripts/optical_original_clearance_inputs.txt'
    $clearanceInputs = @(Get-Content -LiteralPath $clearanceInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$clearanceInputs) { throw 'Original clearance fixture inventory is empty.' }
    $f64Inventory = 'scripts/optical_cartesian_suffix_f64_inputs.txt'
    $f64Inputs = @(Get-Content -LiteralPath $f64Inventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$f64Inputs) { throw 'F64 suffix source and data inventory is empty.' }
    $compositeInventory = 'scripts/optical_cartesian_composite_inputs.txt'
    $compositeInputs = @(Get-Content -LiteralPath $compositeInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$compositeInputs) { throw 'Composite source and data inventory is empty.' }
    $compositeFullInventory = 'scripts/optical_cartesian_composite_full_inputs.txt'
    $compositeFullInputs = @(Get-Content -LiteralPath $compositeFullInventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$compositeFullInputs) { throw 'Complete-frame composite source and data inventory is empty.' }
    $inputFiles = @(
        'Cargo.toml', 'Cargo.lock', $inventory,
        'scripts/verify_optical_wavefront.ps1', 'scripts/optical_wavefront_probe.rs',
        'scripts/optical_cartesian_composite_full_fixture.rs',
        'scripts/optical_midpoint_wavefront_fixture.rs', 'scripts/optical_dispatch_sequence.rs',
        'scripts/optical_wavefront_reduced_fixture.rs', $suffixInventory, $traceInventory, $tightInventory, $branchInventory, $atmosphereInventory, $clearanceInventory, $f64Inventory, $compositeInventory,
        'scripts/optical_gpu_runtime.rs', 'scripts/optical_shader_reachability.rs',
        'scripts/optical_normal_scene_loop_policy.rs'
    ) + $fixtureInputs + $suffixInputs + $traceInputs + $tightInputs + $branchInputs + $atmosphereInputs + $clearanceInputs + $f64Inputs + $compositeInputs + @($compositeFullInventory) + $compositeFullInputs + @($libraries.Values) + $nativeInputs
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
    $executable = Join-Path $projectRoot '.cache/optical-wavefront-probe.exe'
    $compilerArgs = @('--edition=2024', 'scripts/optical_wavefront_probe.rs', '-L', 'dependency=target/debug/deps')
    foreach ($directory in $nativeDirectories) { $compilerArgs += @('-L', "native=$directory") }
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu', 'flate2', 'bevy')) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    $previousManifestDirectory = $env:CARGO_MANIFEST_DIR
    try {
        $env:CARGO_MANIFEST_DIR = $projectRoot
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
    } finally {
        $env:CARGO_MANIFEST_DIR = $previousManifestDirectory
    }
    if ($LASTEXITCODE -ne 0) {
        if (!$CompileOnly) { Get-Content -LiteralPath "$prefix-compiler.log" -Tail 50 }
        throw 'Continuation probe compilation failed.'
    }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'Continuation probe compiled; no execution.'; return }
    # Retain and execute this run's exact binary. The shared compiler output is
    # overwritten by later runs and cannot serve as historical executable proof.
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
        selected = !$Full
        timed = [bool]$Time
        validation_only = [bool]$ValidateOnly
        reference = [bool]$Reference
        reduce_bookkeeping = [bool]$ReduceBookkeeping
        original_clearance = [bool]$OriginalClearance
        cartesian_suffix = [bool]$CartesianSuffix
        cartesian_suffix_trace = [bool]$CartesianSuffixTrace
        cartesian_suffix_tight = [bool]$CartesianSuffixTight
        cartesian_suffix_branch = $CartesianSuffixBranch
        cartesian_suffix_f64 = $CartesianSuffixF64
        cartesian_composite_f64 = $CartesianCompositeF64
        cartesian_composite_reference = [bool]$CartesianCompositeReference
        cartesian_composite_full = [bool]$CartesianCompositeFull
        branch_reference = [bool]$BranchReference
        slice_attempts = if ($CartesianSuffix -or $OriginalClearance -or $CartesianSuffixF64 -or $CartesianCompositeF64 -or $CartesianCompositeFull) { $null } else { $SliceAttempts }
        scope = 'Frozen candidate continuation fixture. Headless dispatch excludes application presentation; offline validation makes no graphics-device calls.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $arguments = @('--name', $Name)
    if ($ValidateOnly) { $arguments += '--validate-only' }
    if ($Full) { $arguments += '--full' }
    if ($Time) { $arguments += '--time' }
    if ($Reference) { $arguments += '--reference' }
    if ($ReduceBookkeeping) { $arguments += '--reduce-bookkeeping' }
    if ($OriginalClearance) { $arguments += '--original-clearance' }
    elseif ($CartesianCompositeFull) { $arguments += '--cartesian-composite-full' }
    elseif ($CartesianSuffixF64) { $arguments += @('--cartesian-suffix-f64', $CartesianSuffixF64) }
    elseif ($CartesianCompositeF64) {
        $arguments += @('--cartesian-composite-f64', $CartesianCompositeF64)
        if ($CartesianCompositeReference) { $arguments += '--cartesian-composite-reference' }
    }
    elseif ($CartesianSuffixBranch) {
        $arguments += @('--cartesian-suffix-branch', $CartesianSuffixBranch)
        if ($BranchReference) { $arguments += '--branch-reference' }
    }
    elseif ($CartesianSuffixTight) { $arguments += '--cartesian-suffix-tight' }
    elseif ($CartesianSuffixTrace) { $arguments += '--cartesian-suffix-trace' }
    elseif ($CartesianSuffix) { $arguments += '--cartesian-suffix' }
    else { $arguments += @('--slice-attempts', $SliceAttempts.ToString()) }
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    $timedOut = $false
    $failure = $null
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                $timedOut = $true
                throw 'Continuation probe exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Continuation probe exited $($process.ExitCode); evidence retained." }
        if (!$ValidateOnly -and !(Test-Path -LiteralPath "$prefix.json")) { throw 'Scene report missing.' }
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
            arguments = $arguments
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
    if (!$inputsStable) { throw 'Inputs changed during execution; comparison is not frozen.' }
    if (!$executableStable) { throw 'Retained executable changed during execution.' }
    Write-Output "Completed $prefix"
} finally {
    Pop-Location
}
