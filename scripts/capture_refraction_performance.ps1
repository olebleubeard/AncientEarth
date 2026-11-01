# Native application timing with explicit focus observations. This does not
# synthesize input, change a ray, hide slow intervals, or unlock the desktop.
param(
    [Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$Name,
    [string]$Executable = 'target/debug/ancient-earth.exe',
    [string[]]$SceneArguments = @(),
    # Separate lifecycle diagnostic: deliberately interrupts warmup. Do not use
    # its output as an uninterrupted startup or performance comparison.
    [switch]$MinimizeDuringWarmup,
    [switch]$TracePrimary,
    [ValidateSet('vulkan', 'dx12')][string]$Backend = 'vulkan',
    [ValidateRange(30, 1200)][int]$TimeoutSeconds = 600
)
$ErrorActionPreference = 'Stop'
$unfocusedOpticalDiagnostic = $SceneArguments -contains '--frame-capture-unfocused-optical'
if ($unfocusedOpticalDiagnostic -and ($SceneArguments -notcontains '--frame-capture-refraction-flight')) {
    throw 'The unfocused optical diagnostic requires the explicit refraction flight probe.'
}
if ($unfocusedOpticalDiagnostic -and $MinimizeDuringWarmup) {
    throw 'Run unfocused motion and minimize/restore diagnostics separately.'
}
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$outputDirectory = Join-Path $projectRoot 'artifacts/performance'
New-Item -ItemType Directory -Path $outputDirectory -Force | Out-Null
$env:WGPU_BACKEND = $Backend
$env:RUST_LOG = if ($MinimizeDuringWarmup -or $TracePrimary) { 'info,ancient_earth::desktop::renderer::refraction=debug' } else { 'info' }
if (-not ('OpticalPerformanceWindow' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class OpticalPerformanceWindow {
    public delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int limit);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    public static IntPtr Find(uint process) {
        IntPtr result = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner != process) return true;
            var title = new StringBuilder(256); GetWindowText(window, title, 256);
            if (title.ToString() != "Ancient Earth | WGS84 Observatory") return true;
            result = window; return false;
        }, IntPtr.Zero);
        return result;
    }
}
'@
}
$stdout = Join-Path $outputDirectory "$Name.stdout.log"
$stderr = Join-Path $outputDirectory "$Name.stderr.log"
$report = Join-Path $outputDirectory "$Name.json"
$launchReport = Join-Path $outputDirectory "$Name.launch.json"
$sourceReport = Join-Path $outputDirectory "$Name.sources.json"
$analysisReport = Join-Path $outputDirectory "$Name.analysis.json"
foreach ($evidencePath in @($stdout, $stderr, $report, $launchReport, $sourceReport, $analysisReport)) {
    if (Test-Path -LiteralPath $evidencePath) {
        throw "Capture evidence already exists: $evidencePath. Use a new name to preserve comparisons."
    }
}
$executablePath = (Resolve-Path -LiteralPath (Join-Path $projectRoot $Executable)).Path
$executableHash = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
# The working tree may intentionally contain uncommitted work. Preserve exact
# source fingerprints beside the executable identity instead of treating HEAD
# as a complete description of the build under test.
$sourcePaths = @('Cargo.toml', 'Cargo.lock', 'licenses/assets.json')
$sourcePaths += @(& rg --files (Join-Path $projectRoot 'src') (Join-Path $projectRoot 'crates') -g '*.rs' -g '*.wesl' -g 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Could not enumerate capture source files.' }
$sourcePaths += @('assets/weather/gfs-regional-reference.grib2', 'assets/weather/hrrr-reference.grib2')
$sourceHashes = foreach ($sourcePath in $sourcePaths) {
    $absoluteSource = if ([IO.Path]::IsPathRooted($sourcePath)) { $sourcePath } else { Join-Path $projectRoot $sourcePath }
    [ordered]@{
        path = [IO.Path]::GetRelativePath($projectRoot, $absoluteSource)
        sha256 = (Get-FileHash -LiteralPath $absoluteSource -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
[ordered]@{
    captured_utc = [DateTime]::UtcNow.ToString('o')
    executable = $Executable
    executable_sha256 = $executableHash
    sources = @($sourceHashes)
    scope = 'Source and verification-weather fingerprints at launch. These are not a source archive or proof of build provenance; the separately hashed executable is the binary actually launched.'
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $sourceReport -Encoding utf8
$arguments = @($SceneArguments) + @('--capture-fullscreen', '--frame-capture', "artifacts/performance/$Name.json")
$started = [DateTime]::UtcNow
$app = Start-Process -FilePath $executablePath -WorkingDirectory $projectRoot -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
$processHandle = $app.Handle
$observations = [System.Collections.Generic.List[object]]::new()
$activationAttempted = $false
$activationAccepted = $false
$complete = $false
$minimizeRequested = $null
$restoreRequested = $null
$minimizedObserved = $false
$suspensionObserved = $false
try {
    while (!$app.WaitForExit(500)) {
        if ([DateTime]::UtcNow - $started -gt [TimeSpan]::FromSeconds($TimeoutSeconds)) {
            throw 'Native refraction performance capture exceeded its watchdog.'
        }
        $appWindow = [OpticalPerformanceWindow]::Find($app.Id)
        if ($appWindow -ne [IntPtr]::Zero) {
            if (!$activationAttempted) {
                # Activate only the application owned by this QA process, once.
                # A rejected request remains visible in the report. Never force
                # focus by attaching to another thread or injecting keystrokes.
                $activationAttempted = $true
                [void][OpticalPerformanceWindow]::ShowWindow($appWindow, 9)
                $activationAccepted = [OpticalPerformanceWindow]::SetForegroundWindow($appWindow)
            }
            if ($MinimizeDuringWarmup) {
                if ($null -eq $minimizeRequested -and (Select-String -LiteralPath $stdout, $stderr -Pattern 'Graphics startup ready' -Quiet) -contains $true) {
                    $minimizeRequested = [DateTime]::UtcNow
                    [void][OpticalPerformanceWindow]::ShowWindow($appWindow, 6)
                }
                if ($null -ne $minimizeRequested -and $null -eq $restoreRequested) {
                    $minimizedObserved = $minimizedObserved -or [OpticalPerformanceWindow]::IsIconic($appWindow)
                    $suspensionObserved = (Select-String -LiteralPath $stdout, $stderr -Pattern 'Withdrew optical compute submission: no drawable window' -Quiet) -contains $true
                    if ([DateTime]::UtcNow - $minimizeRequested -ge [TimeSpan]::FromSeconds(5)) {
                        $restoreRequested = [DateTime]::UtcNow
                        [void][OpticalPerformanceWindow]::ShowWindow($appWindow, 9)
                        [void][OpticalPerformanceWindow]::SetForegroundWindow($appWindow)
                    }
                }
            }
            $foregroundWindow = [OpticalPerformanceWindow]::GetForegroundWindow()
            $observations.Add([pscustomobject]@{
                utc = [DateTime]::UtcNow.ToString('o')
                own_window_is_foreground = ($foregroundWindow -eq $appWindow)
                foreground_window_is_null = ($foregroundWindow -eq [IntPtr]::Zero)
            })
        }
    }
    $app.Refresh()
    if ($app.ExitCode -ne 0) { throw "Native capture failed: exit $($app.ExitCode)." }
    if (!(Test-Path -LiteralPath $report) -or (Get-Item -LiteralPath $report).LastWriteTimeUtc -lt $started) {
        throw 'Native capture did not produce a fresh timing report.'
    }
    if (Select-String -LiteralPath $stderr -Pattern 'Validation Error|DeviceLost|panicked at|timed out|failed to process shader' -Quiet) {
        throw 'Native capture contains graphics/runtime failures; inspect its log.'
    }
    $capture = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if ($capture.metadata.view.weather_refraction_rendered -ne $true -or $capture.final_view.weather_refraction_rendered -ne $true) {
        throw 'Refraction was not ready at both capture boundaries. Raw timing report has been preserved.'
    }
    if (!$unfocusedOpticalDiagnostic -and $capture.unfocused_recorded_frames -ne 0) {
        throw 'Capture lost application focus. Raw timing report has been preserved; do not use it as a focused baseline.'
    }
    if (!$unfocusedOpticalDiagnostic -and $null -ne $capture.optical_motion_qa -and $capture.optical_motion_qa.passed -ne $true) {
        throw 'The optical flight correctness check failed. Raw timing and flight evidence have been preserved.'
    }
    if ($MinimizeDuringWarmup -and (!$minimizedObserved -or !$suspensionObserved -or $null -eq $restoreRequested)) {
        throw 'The native minimize/withdraw/restore sequence was not fully observed. Evidence has been preserved.'
    }
    $complete = $true
} finally {
    if (!$app.HasExited) { $app.Kill(); $app.WaitForExit() }
    [ordered]@{
        started_utc = $started.ToString('o')
        completed_utc = [DateTime]::UtcNow.ToString('o')
        completed = $complete
        process_id = $app.Id
        executable = $Executable
        executable_sha256 = $executableHash
        backend = $Backend
        trace_primary_encoding = [bool]$TracePrimary
        unfocused_optical_diagnostic = $unfocusedOpticalDiagnostic
        arguments = $arguments
        activation_attempted = $activationAttempted
        activation_request_accepted = $activationAccepted
        focus_observations = $observations
        lifecycle_diagnostic = [ordered]@{
            requested = [bool]$MinimizeDuringWarmup
            minimize_requested_utc = if ($null -ne $minimizeRequested) { $minimizeRequested.ToString('o') } else { $null }
            restore_requested_utc = if ($null -ne $restoreRequested) { $restoreRequested.ToString('o') } else { $null }
            native_minimized_observed = $minimizedObserved
            main_world_submission_withdrawal_observed = $suspensionObserved
            scope = 'Warmup-only native window interruption; withdrawal log observes main-world submission removal, not GPU idle. Restored capture readiness and optional exact packet readbacks must be reviewed separately.'
        }
        scope = 'Native foreground-window observations; application report independently records focus, readiness and every interval. No physical input delivery claim.'
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $launchReport -Encoding utf8
}
python (Join-Path $projectRoot 'scripts/analyze_frame_capture.py') $report --output $analysisReport | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Independent frame-capture analysis failed; raw evidence has been preserved.' }
$analysis = @(Get-Content -LiteralPath $analysisReport -Raw | ConvertFrom-Json)[0]
if ($unfocusedOpticalDiagnostic) {
    $capture = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
    if (!$capture.optical_motion_qa.unfocused_diagnostic_policy -or
        !$capture.optical_motion_qa.diagnostic_route_passed -or
        $capture.optical_motion_qa.passed -or
        $analysis.external_foreground_statistics.eligible_for_focused_timing) {
        throw 'Unfocused optical diagnostic did not satisfy its separate route/provenance checks; evidence preserved.'
    }
    Write-Output "Recorded $report; explicit unfocused route diagnostic only, not a focused timing or smooth-flight acceptance."
    return
}
if (!$MinimizeDuringWarmup -and !$analysis.external_foreground_statistics.eligible_for_focused_timing) {
    throw 'OS foreground observations did not verify the recorded timing window. Raw capture and analysis are preserved; do not use this as a focused performance comparison.'
}
Write-Output "Recorded $report; analysis: $analysisReport. Lifecycle evidence and performance eligibility are reported separately."
