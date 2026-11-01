# Compiler diagnostics only; this neither executes rays nor measures FPS.
param(
    [Parameter(Mandatory)][string]$Spirv,
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Name,
    [ValidatePattern('^[A-Za-z_][A-Za-z0-9_]*$')][string]$EntryPoint = 'performance',
    [string]$Backend = '.cache/tools/rga-2.14.2/portable/utils/vulkan/vulkan_backend.exe',
    [string]$PipelineState = 'artifacts/optical-gpu-tools/primary-diagnostic.cpso',
    [ValidateRange(10, 600)][int]$TimeoutSeconds = 120
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$inputPath = (Resolve-Path -LiteralPath (Join-Path $projectRoot $Spirv)).Path
$backendPath = (Resolve-Path -LiteralPath (Join-Path $projectRoot $Backend)).Path
$pipelinePath = (Resolve-Path -LiteralPath (Join-Path $projectRoot $PipelineState)).Path
$outputRoot = Join-Path $projectRoot "artifacts/optical-gpu-tools/$Name"
$outputs = @('.stats', '.isa', '.bin', '.stdout.log', '.stderr.log', '.provenance.json')
foreach ($extension in $outputs) {
    if (Test-Path -LiteralPath "$outputRoot$extension") { throw "Evidence already exists: $outputRoot$extension" }
}
# Vulkan backend auto-selects the installed adapter. An architecture override
# crashed this RGA version on the tested driver. The top-level CLI can silently
# fall back to a different offline compiler; call its backend directly instead.
$arguments = @('--comp', $inputPath, '--comp-entry', $EntryPoint, '--pso', $pipelinePath,
    '--comp-stats', "$outputRoot.stats", '--comp-isa', "$outputRoot.isa", '--bin', "$outputRoot.bin")
$started = [DateTime]::UtcNow
$process = Start-Process -FilePath $backendPath -ArgumentList $arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput "$outputRoot.stdout.log" -RedirectStandardError "$outputRoot.stderr.log"
$processHandle = $process.Handle
$complete = $false
try {
    while (!$process.WaitForExit(500)) {
        if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
            throw 'Compiler resource analysis exceeded its watchdog.'
        }
    }
    $process.Refresh()
    if ($process.ExitCode -ne 0) { throw "Compiler backend failed with exit $($process.ExitCode)." }
    foreach ($extension in @('.stats', '.isa', '.bin')) {
        if (!(Test-Path -LiteralPath "$outputRoot$extension") -or (Get-Item -LiteralPath "$outputRoot$extension").Length -eq 0) {
            throw "Compiler backend did not produce $extension evidence."
        }
    }
    $complete = $true
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $evidence = @($inputPath, $backendPath, $pipelinePath) + @($outputs | Where-Object { $_ -ne '.provenance.json' } | ForEach-Object { "$outputRoot$_" } | Where-Object { Test-Path -LiteralPath $_ })
    [ordered]@{
        completed = $complete
        started_utc = $started.ToString('o')
        finished_utc = [DateTime]::UtcNow.ToString('o')
        arguments = $arguments
        files = @($evidence | ForEach-Object { [ordered]@{path=[IO.Path]::GetRelativePath($projectRoot, $_);sha256=(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()} })
        scope = 'Compiler resource analysis of the supplied diagnostic SPIR-V and pipeline layout, using automatic installed-adapter selection. Not a capture of the application shader binary, ray execution, or frame timing. Preserve the SPIR-V export policy separately.'
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$outputRoot.provenance.json" -Encoding utf8
}
Get-Content -LiteralPath "$outputRoot.stats"
