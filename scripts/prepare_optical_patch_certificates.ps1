param(
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Name,
    [switch]$CompileOnly,
    [ValidateRange(1, 600)][int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (!$CompileOnly -and (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue)) {
        throw 'Preparation evidence already exists; choose a new name.'
    }
    $libraries = @{}
    foreach ($crate in @('earth_math', 'serde_json', 'flate2', 'wgpu')) {
        $item = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Rebuild current desktop dependencies first: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $inputFiles = @(
        'Cargo.toml', 'Cargo.lock',
        'scripts/prepare_optical_patch_certificates.rs', 'scripts/prepare_optical_patch_certificates.ps1',
        'scripts/optical_patch_certificates.rs',
        'scripts/optical_gpu_probe.rs', 'scripts/optical_shader_variants.rs',
        'scripts/optical_taylor.rs', 'scripts/optical_normal_ideal_frame.rs',
        'artifacts/optical-positive-contact-packet-first-faces.bin',
        'artifacts/optical-requested-stellar-sparse-chain-geometry.bin',
        'artifacts/optical-requested-stellar-sparse-chain-frames.bin',
        '.cache/topography-wide-area-qa/selection.json',
        'assets/geoid/egm2008-native.aeg.gz',
        'assets/boundaries/region-01.aeb', 'assets/boundaries/region-02.aeb',
        'assets/boundaries/region-03.aeb', 'assets/boundaries/region-04.aeb',
        'assets/weather/gfs-regional-reference.grib2', 'assets/weather/hrrr-reference.grib2'
    ) + @(Get-ChildItem -LiteralPath '.cache/topography-wide-area-qa/assets' -Filter '*.tif' -File | Sort-Object Name | ForEach-Object FullName) + @($libraries.Values | Sort-Object)
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
    $executable = Join-Path $projectRoot '.cache/prepare-optical-patch-certificates.exe'
    $compilerArgs = @('--edition=2024', '-O', 'scripts/prepare_optical_patch_certificates.rs', '-L', 'dependency=target/debug/deps')
    foreach ($crate in @('earth_math', 'serde_json', 'flate2', 'wgpu')) {
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
        throw 'Patch certificate preparation compilation failed.'
    }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Preparation inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'CPU patch certificate preparer compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        compiler_arguments = $compilerArgs
        compiler_version = (& rustc --version)
        scope = 'CPU-only immutable terrain certificates and per-eye patch metadata. No ray execution, GPU upload or native performance evidence.'
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
                throw 'CPU patch certificate preparation exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "CPU patch certificate preparation exited $($process.ExitCode); evidence retained." }
        foreach ($suffix in @('.json', '-faces.bin', '-geometry.bin', '-eye-0.bin', '-eye-1.bin', '-eye-2.bin')) {
            if (!(Test-Path -LiteralPath "$prefix$suffix")) { throw "Missing patch packet output: $suffix" }
        }
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
            arguments = @($prefix)
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
    if (!$inputsStable) { throw 'Preparation inputs changed during execution; evidence is not frozen.' }
    Write-Output "Prepared $prefix; independent packet review required."
} finally {
    Pop-Location
}
