param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-normal-packet',
    [switch]$CompileOnly,
    [ValidateRange(1, 300)][int]$TimeoutSeconds = 180
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
    foreach ($crate in @('earth_math', 'serde_json', 'bevy')) {
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
    $inputFiles = @(
        'Cargo.toml', 'Cargo.lock',
        'scripts/optical_normal_packet_probe.rs', 'scripts/optical_normal_packet_fixture.rs',
        'scripts/verify_optical_normal_packet.ps1', 'src/desktop/renderer/refraction/normal.rs',
        'artifacts/optical-normal-coordinate-current.json',
        'artifacts/optical-height-trace-fast-metric-uniform.bin',
        'artifacts/optical-normal-packet-encase-first.stderr.log'
    ) + @($libraries.Values | Sort-Object) + @($nativeInputs | Sort-Object -Unique)
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
    $executable = Join-Path $projectRoot '.cache/optical-normal-packet-probe.exe'
    $compilerArgs = @('--edition=2024', 'scripts/optical_normal_packet_probe.rs', '-L', 'dependency=target/debug/deps')
    foreach ($directory in $nativeDirectories) { $compilerArgs += @('-L', "native=$directory") }
    foreach ($crate in @('earth_math', 'serde_json', 'bevy')) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    $previousManifestDirectory = $env:CARGO_MANIFEST_DIR
    try {
        # Bevy's ShaderType derive locates its re-export through this manifest.
        $env:CARGO_MANIFEST_DIR = $projectRoot
        & rustc @compilerArgs
    } finally {
        $env:CARGO_MANIFEST_DIR = $previousManifestDirectory
    }
    if ($LASTEXITCODE -ne 0) { throw 'Candidate packet ABI probe compilation failed.' }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Diagnostic inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'Candidate packet ABI probe compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        scope = 'CPU-only candidate uniform ABI. Source, retained canonical fixture, linked-library and executable fingerprints; no GPU execution or fresh weather-import equality claim.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList @('--name', $Name) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                throw 'Candidate packet ABI probe exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Candidate packet ABI probe exited $($process.ExitCode); evidence retained." }
        if (!(Test-Path -LiteralPath "$prefix.json")) { throw 'Candidate packet ABI report missing.' }
    } finally {
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
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
    if (!$inputsStable) { throw 'Diagnostic inputs changed during execution; comparison is not frozen.' }
    Write-Output "Wrote $prefix.json"
} finally {
    Pop-Location
}
