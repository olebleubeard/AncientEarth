# Original MIT. Mechanical runner for the mathematics-owned CPU feasibility screen.
param(
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Name,
    [ValidateRange(1, 600)][int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $projectRoot
try {
    $prefix = "artifacts/$Name"
    if (Get-ChildItem -LiteralPath artifacts -Filter "$Name*" -ErrorAction SilentlyContinue) {
        throw 'Evidence already exists; choose a new name.'
    }
    $entry = 'scripts/optical_patch_relative_feasibility.py'
    $inventory = 'scripts/optical_patch_relative_inputs.txt'
    $fixtureInputs = @(Get-Content -LiteralPath $inventory | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
    if (!$fixtureInputs) { throw 'Feasibility source and data inventory is empty.' }
    $runtimeProbe = @'
import json, platform, sys
import numpy, mpmath
import numpy._core._multiarray_umath as core
print(json.dumps({
    "executable": sys.executable,
    "python_version": platform.python_version(),
    "numpy_version": numpy.__version__,
    "mpmath_version": mpmath.__version__,
    "files": [sys.executable, numpy.__file__, core.__file__, mpmath.__file__],
    "scope": "Interpreter and selected numerical module fingerprints; not a complete Python installation audit."
}))
'@
    $runtimeText = & python -B -c $runtimeProbe
    if ($LASTEXITCODE -ne 0) { throw 'Python numerical runtime inspection failed.' }
    $runtime = $runtimeText | ConvertFrom-Json
    $inputFiles = @(
        $entry, $inventory, 'scripts/verify_optical_patch_relative.ps1'
    ) + $fixtureInputs + @($runtime.files)
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
    $beforeRun = @(Get-InputFingerprints)
    [ordered]@{
        created_utc = [DateTime]::UtcNow.ToString('o')
        files = $beforeRun
        runtime = $runtime
        scope = 'CPU coordinate-certificate feasibility only. No GPU execution, native flight timing or production acceptance.'
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath "$prefix-sources.json" -Encoding utf8
    $arguments = @('-B', $entry, $prefix)
    $started = [DateTime]::UtcNow
    $process = Start-Process -FilePath $runtime.executable -ArgumentList $arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot "$prefix.stdout.log") -RedirectStandardError (Join-Path $projectRoot "$prefix.stderr.log")
    $processHandle = $process.Handle
    $timedOut = $false
    $failure = $null
    try {
        while (!$process.WaitForExit(500)) {
            if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
                $timedOut = $true
                throw 'CPU feasibility screen exceeded its bounded timeout.'
            }
        }
        $process.Refresh()
        if ($process.ExitCode -ne 0) { throw "CPU feasibility screen exited $($process.ExitCode); evidence retained." }
        if (!(Test-Path -LiteralPath "$prefix.json")) { throw 'Feasibility report is missing.' }
    } catch {
        $failure = $_.Exception.Message
        throw
    } finally {
        $terminatedByWrapper = !$process.HasExited
        if ($terminatedByWrapper) { $process.Kill(); $process.WaitForExit() }
        $process.Refresh()
        [ordered]@{
            owned_process_id = $process.Id
            executable = $runtime.executable
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
        $inputsStable = ($beforeRun | ConvertTo-Json -Compress) -eq ($afterRun | ConvertTo-Json -Compress)
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
    if (!$inputsStable) { throw 'Feasibility inputs changed during execution; evidence is not frozen.' }
    Write-Output "Completed $prefix; independent mathematical review required."
} finally {
    Pop-Location
}
