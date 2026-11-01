param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-normal-scene-preparation',
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
        'scripts/prepare_optical_normal_scene.rs', 'scripts/prepare_optical_normal_scene.ps1',
        'scripts/optical_gpu_probe.rs', 'scripts/optical_shader_variants.rs',
        'scripts/optical_taylor.rs', 'scripts/optical_normal_map_packet.rs',
        'scripts/optical_normal_ideal_frame.rs', 'scripts/optical_scene_ceiling.rs',
        'artifacts/optical-normal-coordinate-current.json',
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
    $executable = Join-Path $projectRoot '.cache/prepare-optical-normal-scene.exe'
    $compilerArgs = @('--edition=2024', 'scripts/prepare_optical_normal_scene.rs', '-L', 'dependency=target/debug/deps')
    foreach ($crate in @('earth_math', 'serde_json', 'flate2', 'wgpu')) {
        $compilerArgs += @('--extern', "$crate=$($libraries[$crate])")
    }
    $compilerArgs += @('-o', $executable)
    & rustc @compilerArgs
    if ($LASTEXITCODE -ne 0) { throw 'Scene preparation compilation failed.' }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Preparation inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'CPU scene preparer compiled; no execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        scope = 'CPU-only immutable canonical scene preparation; no ray execution or GPU performance evidence.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                throw 'CPU scene preparation exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "CPU scene preparation exited $($process.ExitCode); evidence retained." }
        if (!(Test-Path -LiteralPath artifacts/optical-normal-scene-packet.json)) { throw 'Scene packet manifest missing.' }
    } finally {
        if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $afterRun = @(Get-InputFingerprints)
        $inputsStable = ($beforeCompilation | ConvertTo-Json -Compress) -eq ($afterRun | ConvertTo-Json -Compress)
        [ordered]@{
            inputs_unchanged_through_run = $inputsStable
            after_run = $afterRun
        } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-input-stability.json" -Encoding utf8
        $outputs = @(
            Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -File
            Get-ChildItem -LiteralPath artifacts -Filter 'optical-normal-scene-packet*' -File
        ) | Sort-Object FullName -Unique | Where-Object { $_.Name -ne "$Name-outputs.json" }
        @($outputs | ForEach-Object {
            [ordered]@{
                path = [IO.Path]::GetRelativePath($projectRoot, $_.FullName)
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }) | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath "$prefix-outputs.json" -Encoding utf8
    }
    if (!$inputsStable) { throw 'Preparation inputs changed during execution; evidence is not frozen.' }
    Write-Output 'Prepared immutable scene packet; independent numerical review required.'
} finally {
    Pop-Location
}
