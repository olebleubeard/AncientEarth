param(
    [ValidatePattern('^[a-z0-9-]+$')][string]$Name = 'optical-normal-query',
    [switch]$CompileOnly,
    [ValidateRange(1, 300)][int]$TimeoutSeconds = 120
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
    foreach ($crate in @('earth_math', 'serde_json', 'wgpu')) {
        $item = Get-ChildItem -LiteralPath target/debug/deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$item) { throw "Rebuild current desktop dependencies first: $crate" }
        $libraries[$crate] = $item.FullName
    }
    $sourcePaths = @(
        'scripts/optical_normal_query_probe.rs', 'scripts/optical_normal_step.wgsl',
        'scripts/generate_optical_normal_step.py', 'scripts/optical_normal_coordinates.rs',
        'scripts/verify_optical_normal_queries.ps1'
    )
    $inputFiles = @($sourcePaths) + @($libraries.Values | Sort-Object)
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
    $executable = Join-Path $projectRoot '.cache/optical-normal-query-probe.exe'
    & rustc --edition=2024 scripts/optical_normal_query_probe.rs -L dependency=target/debug/deps --extern "earth_math=$($libraries['earth_math'])" --extern "serde_json=$($libraries['serde_json'])" --extern "wgpu=$($libraries['wgpu'])" -o $executable
    if ($LASTEXITCODE -ne 0) { throw 'Normal-chart query compilation failed.' }
    $afterCompilation = @(Get-InputFingerprints)
    if (($beforeCompilation | ConvertTo-Json -Compress) -ne ($afterCompilation | ConvertTo-Json -Compress)) {
        throw 'Diagnostic inputs changed during compilation; do not run this build.'
    }
    if ($CompileOnly) { Write-Output 'Normal-chart probe compiled; no GPU execution.'; return }
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeCompilation + @([ordered]@{
            path = [IO.Path]::GetRelativePath($projectRoot, $executable)
            sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
        })
        compiler_inputs_unchanged = $true
        scope = 'Exact fixture, source, linked-library and executable fingerprints for bounded normal-coordinate field/trial validation. Not application frame-rate evidence.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $executable -WorkingDirectory $projectRoot -ArgumentList @('--name', $Name) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                throw 'Normal-chart query exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "Normal-chart query exited $($process.ExitCode); raw evidence retained." }
        if (!(Test-Path -LiteralPath "$prefix.json")) { throw 'Normal-chart report missing.' }
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
    if (!$inputsStable) { throw 'Diagnostic inputs changed while running; preserved evidence is not a frozen-input comparison.' }
    Write-Output "Wrote $prefix.json"
} finally {
    Pop-Location
}
