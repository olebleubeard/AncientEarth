param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-normal-scene',
    [switch]$CompileOnly,
    [switch]$ValidateOnly,
    [switch]$Full,
    [switch]$Time,
    [switch]$Reference,
    [switch]$ReferenceReuse,
    [switch]$ReferenceCarry,
    [switch]$Curves,
    [switch]$CartesianCurves,
    [switch]$CartesianCovered,
    [switch]$CartesianStages,
    [switch]$PositiveContacts,
    [switch]$PositiveStages,
    [switch]$PositiveChronology,
    [switch]$PruneUnused,
    [switch]$UniformLoops,
    [switch]$FiniteLoops,
    [ValidateRange(1, 900)][int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
if ($Time -and !$Full) { throw '-Time requires -Full.' }
if (($Curves -or $CartesianCurves -or $CartesianCovered -or $CartesianStages -or $PositiveContacts -or $PositiveStages -or $PositiveChronology) -and ($Full -or $Time -or $Reference -or $ReferenceReuse -or $ReferenceCarry)) { throw 'Curve fixtures are isolated interval diagnostics.' }
if (([int][bool]$Curves + [int][bool]$CartesianCurves + [int][bool]$CartesianCovered + [int][bool]$CartesianStages + [int][bool]$PositiveContacts + [int][bool]$PositiveStages + [int][bool]$PositiveChronology) -gt 1) { throw 'Choose one interval fixture.' }
if (([int][bool]$Reference + [int][bool]$ReferenceReuse + [int][bool]$ReferenceCarry) -gt 1) { throw 'Choose one reference implementation.' }
if ($UniformLoops -and (!($Curves -or $CartesianCurves -or $CartesianCovered -or $CartesianStages -or $PositiveContacts -or $PositiveStages -or $PositiveChronology) -or !$PruneUnused)) { throw '-UniformLoops requires an isolated interval fixture and -PruneUnused.' }
if ($FiniteLoops -and (!$Reference -or !$Full -or $PruneUnused -or $UniformLoops)) { throw '-FiniteLoops requires the unchanged -Reference -Full fixture.' }
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
    $inputFiles = @(
        'Cargo.toml', 'Cargo.lock', $inventory,
        'scripts/verify_optical_normal_scene.ps1', 'scripts/optical_normal_scene_probe.rs',
        'scripts/optical_gpu_runtime.rs', 'scripts/optical_shader_reachability.rs',
        'scripts/optical_normal_scene_loop_policy.rs',
        'scripts/optical_primary_termination.rs', 'scripts/optical_primary_termination.json',
        'artifacts/optical-midpoint-reference-full.wgsl',
        'artifacts/optical-midpoint-reference-full-primary.bin'
    ) + $fixtureInputs + @($libraries.Values) + $nativeInputs
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
    $executable = Join-Path $projectRoot '.cache/optical-normal-scene-probe.exe'
    $compilerArgs = @('--edition=2024', 'scripts/optical_normal_scene_probe.rs', '-L', 'dependency=target/debug/deps')
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
        throw 'Scene probe compilation failed.'
    }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'Scene probe compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        selected = !$Full -and !$Curves -and !$CartesianCurves -and !$CartesianCovered -and !$CartesianStages -and !$PositiveContacts -and !$PositiveStages -and !$PositiveChronology
        timed = [bool]$Time
        validation_only = [bool]$ValidateOnly
        reference = [bool]$Reference
        reference_reuse = [bool]$ReferenceReuse
        reference_carry = [bool]$ReferenceCarry
        curves = [bool]$Curves
        cartesian_curves = [bool]($CartesianCurves -or $CartesianStages -or $PositiveContacts -or $PositiveStages -or $PositiveChronology)
        cartesian_covered = [bool]($CartesianCovered -or $CartesianStages -or $PositiveContacts -or $PositiveStages -or $PositiveChronology)
        cartesian_stages = [bool]$CartesianStages
        positive_contacts = [bool]($PositiveContacts -or $PositiveStages -or $PositiveChronology)
        positive_stages = [bool]$PositiveStages
        positive_chronology = [bool]$PositiveChronology
        prune_unused = [bool]$PruneUnused
        uniform_loops = [bool]$UniformLoops
        finite_loops = [bool]$FiniteLoops
        scope = 'Frozen candidate scene fixture. Headless dispatch excludes application presentation; offline validation makes no graphics-device calls.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $arguments = @('--name', $Name)
    if ($ValidateOnly) { $arguments += '--validate-only' }
    if ($Full) { $arguments += '--full' }
    if ($Time) { $arguments += '--time' }
    if ($Reference) { $arguments += '--reference' }
    if ($ReferenceReuse) { $arguments += '--reference-reuse' }
    if ($ReferenceCarry) { $arguments += '--reference-carry' }
    if ($Curves) { $arguments += '--curves' }
    if ($CartesianCurves) { $arguments += '--cartesian-curves' }
    if ($CartesianCovered) { $arguments += '--cartesian-covered' }
    if ($CartesianStages) { $arguments += '--cartesian-stages' }
    if ($PositiveContacts) { $arguments += '--positive-contacts' }
    if ($PositiveStages) { $arguments += '--positive-stages' }
    if ($PositiveChronology) { $arguments += '--positive-chronology' }
    if ($PruneUnused) { $arguments += '--prune-unused' }
    if ($UniformLoops) { $arguments += '--uniform-loops' }
    if ($FiniteLoops) { $arguments += '--finite-loops' }
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    $timedOut = $false
    $failure = $null
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                $timedOut = $true
                throw 'Scene probe exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Scene probe exited $($process.ExitCode); evidence retained." }
        if (!$ValidateOnly -and !(Test-Path -LiteralPath "$prefix.json")) { throw 'Scene report missing.' }
    } catch {
        $failure = $_.Exception.Message
        throw
    } finally {
        $terminatedByWrapper = !$process.HasExited
        if ($terminatedByWrapper) { $process.Kill(); $process.WaitForExit() }
        $process.Refresh()
        [ordered]@{
            owned_process_id = $process.Id
            executable = $executable
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
    Write-Output "Completed $prefix"
} finally {
    Pop-Location
}
