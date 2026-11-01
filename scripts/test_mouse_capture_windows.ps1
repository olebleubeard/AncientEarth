# Opt-in native integration check. Requires an interactive Windows desktop.
# It activates only its own app window, then closes it and restores the cursor.
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$env:WGPU_BACKEND = 'dx12'
$env:RUST_LOG = 'info'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class FlightCursorQa {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x; public int y; }
    [StructLayout(LayoutKind.Sequential)] public struct CursorInfo {
        public int size; public int flags; public IntPtr handle; public Point position;
    }
    [DllImport("user32.dll")] public static extern bool GetCursorInfo(ref CursorInfo value);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point value);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    public delegate bool EnumCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int limit);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr window);
    public static void Activate(IntPtr window) {
        uint unused; uint foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out unused);
        uint caller = GetCurrentThreadId();
        bool attached = foregroundThread != caller && AttachThreadInput(caller, foregroundThread, true);
        try { ShowWindow(window, 9); BringWindowToTop(window); SetForegroundWindow(window); }
        finally { if (attached) AttachThreadInput(caller, foregroundThread, false); }
    }
    public static string Describe(IntPtr window) {
        uint owner; GetWindowThreadProcessId(window, out owner);
        StringBuilder title = new StringBuilder(256); GetWindowText(window, title, 256);
        return window + " PID=" + owner + " Title=" + title.ToString();
    }
    public static IntPtr ApplicationWindow(uint process) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, parameter) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            StringBuilder title = new StringBuilder(256); GetWindowText(window, title, 256);
            if (owner == process && title.ToString() == "Ancient Earth | WGS84 Observatory") {
                found = window; return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    public static bool Visible() {
        CursorInfo value = new CursorInfo(); value.size = Marshal.SizeOf(typeof(CursorInfo));
        if (!GetCursorInfo(ref value)) throw new Exception("GetCursorInfo failed");
        return (value.flags & 1) != 0;
    }
}
'@
$originalCursor = [FlightCursorQa+Point]::new()
[void][FlightCursorQa]::GetCursorPos([ref]$originalCursor)
$qaStarted = [DateTime]::UtcNow.ToString('o')
$reportPath = Join-Path $projectRoot 'artifacts/continuous-input-qa.json'
@{ passed = $false; status = 'running'; started_utc = $qaStarted } | ConvertTo-Json | Set-Content -LiteralPath $reportPath -Encoding utf8
$launched = Start-Process -FilePath (Join-Path $projectRoot 'target/debug/ancient-earth.exe') -WorkingDirectory $projectRoot -ArgumentList @('--datetime','2026-10-21T18:00:00','--paused','--clean-view','--no-grass') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $projectRoot 'artifacts/continuous-input.stdout.log') -RedirectStandardError (Join-Path $projectRoot 'artifacts/continuous-input.stderr.log')
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do {
        Start-Sleep -Milliseconds 250
        $launched.Refresh()
        if ($launched.HasExited) { throw 'Application exited before input QA' }
        $appWindow = [FlightCursorQa]::ApplicationWindow($launched.Id)
    } until ($appWindow -ne [IntPtr]::Zero -or [DateTime]::UtcNow -gt $deadline)
    if ($appWindow -eq [IntPtr]::Zero) { throw 'No native application window' }
    # Flight is deliberately disabled until the first coherent scene is ready.
    # Wait for that event instead of guessing how long this driver will compile.
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(120)
    do {
        Start-Sleep -Milliseconds 250
        $launched.Refresh()
        if ($launched.HasExited) { throw 'Application exited during graphics preparation' }
        $graphicsReady = Select-String -LiteralPath (Join-Path $projectRoot 'artifacts/continuous-input.stderr.log'), (Join-Path $projectRoot 'artifacts/continuous-input.stdout.log') -Pattern 'Graphics startup ready' -Quiet
    } until ($graphicsReady -contains $true -or [DateTime]::UtcNow -gt $readyDeadline)
    if (!($graphicsReady -contains $true)) { throw 'Graphics did not become ready for input QA' }
    # This is the interactive native-input QA window owned by this script.
    # The process starts hidden; explicitly restore it before testing OS input.
    [void][FlightCursorQa]::ShowWindow($appWindow, 9)
    $shell = New-Object -ComObject WScript.Shell
    if (!$shell.AppActivate($launched.Id)) { throw 'Could not activate the QA application' }
    [FlightCursorQa]::Activate($appWindow)
    $clickPoint = [FlightCursorQa+Point]::new()
    $clickPoint.x = 960
    $clickPoint.y = 500
    if (![FlightCursorQa]::ClientToScreen($appWindow, [ref]$clickPoint)) { throw 'ClientToScreen failed' }
    [void][FlightCursorQa]::SetCursorPos($clickPoint.x, $clickPoint.y)
    Start-Sleep -Seconds 1
    $foregroundMatches = [FlightCursorQa]::GetForegroundWindow() -eq $appWindow
    $pointerMatches = [FlightCursorQa]::WindowFromPoint($clickPoint) -eq $appWindow
    if (!$foregroundMatches -or !$pointerMatches) {
        Write-Output ([FlightCursorQa]::Describe($appWindow))
        Write-Output ([FlightCursorQa]::Describe([FlightCursorQa]::GetForegroundWindow()))
        Write-Output ([FlightCursorQa]::Describe([FlightCursorQa]::WindowFromPoint($clickPoint)))
        throw "QA focus/pointer is not on the launched window (focus=$foregroundMatches; pointer=$pointerMatches; point=$($clickPoint.x),$($clickPoint.y))"
    }
    $before = [FlightCursorQa]::Visible()
    [FlightCursorQa]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    [FlightCursorQa]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Seconds 2
    $captured = [FlightCursorQa]::Visible()
    [FlightCursorQa]::keybd_event(27,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    [FlightCursorQa]::keybd_event(27,0,2,[UIntPtr]::Zero)
    Start-Sleep -Seconds 2
    $released = [FlightCursorQa]::Visible()
    $report = [ordered]@{
        started_utc = $qaStarted
        status = 'complete'
        scope = 'Native Windows cursor visibility after background click/release and Escape; no movement timing claim'
        cursor_visible_before = $before
        cursor_visible_after_click_and_release = $captured
        cursor_visible_after_escape = $released
        verified_target_window = $foregroundMatches -and $pointerMatches
        passed = ($before -and !$captured -and $released)
    }
    $report | ConvertTo-Json | Set-Content -LiteralPath $reportPath -Encoding utf8
    if (!$report.passed) { throw 'Native click capture/Escape release failed' }
    Write-Output 'Native click capture and Escape release passed.'
} catch {
    @{
        passed = $false
        status = 'failed'
        started_utc = $qaStarted
        error = $_.Exception.Message
    } | ConvertTo-Json | Set-Content -LiteralPath $reportPath -Encoding utf8
    throw
} finally {
    [FlightCursorQa]::keybd_event(27,0,2,[UIntPtr]::Zero)
    if (!$launched.HasExited) {
        if ($appWindow -ne [IntPtr]::Zero) { [void][FlightCursorQa]::PostMessage($appWindow, 16, [IntPtr]::Zero, [IntPtr]::Zero) }
        if (!$launched.WaitForExit(10000)) { $launched.Kill() }
    }
    [void][FlightCursorQa]::SetCursorPos($originalCursor.x, $originalCursor.y)
}
