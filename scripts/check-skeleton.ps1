param(
    [ValidateSet('FirstBatch', 'Full')][string]$Suite = 'FirstBatch',
    [string]$SessionProof
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $IsWindows) { throw 'The Rooom resident gate requires Windows.' }
if ((Get-Process -Id $PID).SessionId -eq 0) { throw 'Run the resident gate in a Windows desktop session.' }

if (-not ('RooomWindowProof' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Threading;

public static class RooomWindowProof {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential)]
    struct MonitorInfo { public uint Size; public Rect Monitor, Work; public uint Flags; }

    public struct Evidence { public Rect Bounds, Work; public uint OwnerPid; public bool Visible; }

    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll", SetLastError = true)] public static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint key);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool UnregisterHotKey(IntPtr window, int id);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", SetLastError = true)] static extern bool GetWindowRect(IntPtr window, out Rect bounds);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);

    // Windows permits a debugged process to activate its own window. Attach
    // only to the disposable resident during the explicit-focus check.
    public sealed class FocusDebugScope : IDisposable {
        [DllImport("kernel32.dll", SetLastError = true)] static extern bool DebugActiveProcess(uint pid);
        [DllImport("kernel32.dll")] static extern bool DebugActiveProcessStop(uint pid);
        [DllImport("kernel32.dll")] static extern bool DebugSetProcessKillOnExit(bool kill);
        [DllImport("kernel32.dll")] static extern bool WaitForDebugEvent(IntPtr record, uint timeout);
        [DllImport("kernel32.dll")] static extern bool ContinueDebugEvent(uint pid, uint tid, uint disposition);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);

        readonly ManualResetEventSlim ready = new ManualResetEventSlim();
        readonly Thread worker;
        volatile bool stop;
        int attachError;

        public FocusDebugScope(uint pid) {
            worker = new Thread(() => Pump(pid)) { IsBackground = true };
            worker.Start();
            ready.Wait();
            if (attachError != 0) {
                Dispose();
                throw new Win32Exception(attachError);
            }
        }

        void Pump(uint pid) {
            if (!DebugActiveProcess(pid)) {
                attachError = Marshal.GetLastWin32Error();
                ready.Set();
                return;
            }
            DebugSetProcessKillOnExit(false);
            // DEBUG_EVENT's union starts at byte 16 under the x64 Windows ABI.
            IntPtr record = Marshal.AllocHGlobal(256);
            try {
                ready.Set();
                while (!stop) {
                    if (!WaitForDebugEvent(record, 75)) continue;
                    int kind = Marshal.ReadInt32(record, 0);
                    uint process = (uint)Marshal.ReadInt32(record, 4);
                    uint thread = (uint)Marshal.ReadInt32(record, 8);
                    uint disposition = 0x00010002; // DBG_CONTINUE
                    if (kind == 1 && (uint)Marshal.ReadInt32(record, 16) != 0x80000003) {
                        disposition = 0x80010001; // DBG_EXCEPTION_NOT_HANDLED
                    }
                    if (kind == 3 || kind == 6) {
                        IntPtr file = Marshal.ReadIntPtr(record, 16);
                        if (file != IntPtr.Zero) CloseHandle(file);
                    }
                    ContinueDebugEvent(process, thread, disposition);
                }
            } finally {
                DebugActiveProcessStop(pid);
                Marshal.FreeHGlobal(record);
            }
        }

        public void Dispose() {
            stop = true;
            worker.Join();
            ready.Dispose();
        }
    }

    public static Evidence Read(IntPtr window) {
        IntPtr old = SetThreadDpiAwarenessContext(new IntPtr(-4));
        try {
            var result = new Evidence { Visible = IsWindowVisible(window) };
            GetWindowThreadProcessId(window, out result.OwnerPid);
            if (!GetWindowRect(window, out result.Bounds)) throw new Win32Exception();
            var info = new MonitorInfo { Size = (uint) Marshal.SizeOf<MonitorInfo>() };
            if (!GetMonitorInfo(MonitorFromWindow(window, 2), ref info)) throw new Win32Exception();
            result.Work = info.Work;
            return result;
        } finally {
            if (old != IntPtr.Zero) SetThreadDpiAwarenessContext(old);
        }
    }
}
'@
}

function Start-Skeleton([string]$Directory, [switch]$Full, [switch]$Health) {
    $environment = @{
        ROOOM_SMOKE_ROOT = $Directory
        ROOOM_SMOKE_ID = (Split-Path (Split-Path $Directory -Parent) -Leaf) + '-' + (Split-Path $Directory -Leaf)
        ROOOM_SMOKE_FULL = if ($Full) { '1' } else { $null }
    }
    $options = @{
        FilePath = $exe
        WorkingDirectory = $Directory
        WindowStyle = 'Hidden'
        Environment = $environment
        PassThru = $true
    }
    if ($Health) {
        $options.ArgumentList = '--health'
        $options.RedirectStandardOutput = Join-Path $Directory 'health.txt'
    }
    Start-Process @options
}

function Wait-Receipt($Process, [string]$Directory, [int]$Revision) {
    $path = Join-Path $Directory "$Revision.json"
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt 45) {
        if ($Process.HasExited) { throw "Rooom exited before WebView receipt $Revision with code $($Process.ExitCode)." }
        try {
            if (Test-Path -LiteralPath $path) {
                return (Get-Content -LiteralPath $path -Raw | ConvertFrom-Json)
            }
        } catch [System.IO.IOException] {}
        Start-Sleep -Milliseconds 80
    }
    throw "Timed out waiting for WebView receipt $Revision."
}

function Assert-Window($Process, $Receipt, [string]$Surface, [string]$Edge, [int]$Affinity) {
    if ($Receipt.snapshot.surface -cne $Surface -or $Receipt.snapshot.settings.edge -cne $Edge) {
        throw "Unexpected surface or settings in the WebView receipt for $Surface."
    }
    if ($Receipt.affinity -ne $Affinity -or -not $Receipt.tray) {
        throw "Missing tray or unexpected display affinity for $Surface."
    }
    $native = [RooomWindowProof]::Read([IntPtr]::new([long]$Receipt.hwnd))
    $bounds = $native.Bounds
    $work = $native.Work
    if (-not $native.Visible -or $native.OwnerPid -ne $Process.Id) {
        throw "The reported $Surface window was not visible and owned by this process."
    }
    $actualWidth = $bounds.Right - $bounds.Left
    $actualHeight = $bounds.Bottom - $bounds.Top
    $scale = [double]$Receipt.scaleFactor
    if ($Surface -eq 'panel') { $width = 380; $height = 210 }
    elseif ($Edge -in @('top', 'bottom')) { $width = 72; $height = 14 }
    else { $width = 14; $height = 72 }
    $expectedWidth = [Math]::Min([Math]::Round($width * $scale), $work.Right - $work.Left)
    $expectedHeight = [Math]::Min([Math]::Round($height * $scale), $work.Bottom - $work.Top)
    if ([Math]::Abs($actualWidth - $expectedWidth) -gt 2 -or [Math]::Abs($actualHeight - $expectedHeight) -gt 2) {
        throw "Unexpected $Surface size: $($actualWidth)x$actualHeight; expected $($expectedWidth)x$expectedHeight."
    }
    $distance = switch ($Edge) {
        'right' { $work.Right - $bounds.Right }
        'bottom' { $work.Bottom - $bounds.Bottom }
        default { throw "Unexpected test edge $Edge." }
    }
    if ([Math]::Abs($distance) -gt 2) { throw "The $Surface did not meet the $Edge work-area edge." }
}

function Invoke-CleanProfile([string]$Directory, [string]$Settings, [string]$Edge, [int]$Affinity, [bool]$Invalid) {
    New-Item -ItemType Directory -Path $Directory | Out-Null
    $settingsPath = Join-Path $Directory 'settings.json'
    if ($Settings) { [IO.File]::WriteAllText($settingsPath, $Settings) }
    $process = Start-Skeleton $Directory
    try {
        $strip = Wait-Receipt $process $Directory 0
        Assert-Window $process $strip 'strip' $Edge $Affinity
        $issue = if ($Invalid) { 'invalid' } else { $null }
        if ($strip.snapshot.settingsIssue -cne $issue) { throw 'Unexpected settings issue.' }
        if ($Edge -eq 'bottom' -and ($strip.snapshot.settings.display -cne 'Fixture display' -or
            $strip.snapshot.settings.shortcut -cne 'Ctrl+Alt+R')) { throw 'Settings did not reach the snapshot.' }

        [IO.File]::WriteAllText((Join-Path $Directory 'open.request'), '')
        $panel = Wait-Receipt $process $Directory 1
        Assert-Window $process $panel 'panel' $Edge $Affinity
        if ($panel.hwnd -ne $strip.hwnd -or $panel.warningVisible -ne $Invalid) {
            throw 'The panel did not reuse the strip window or report the settings issue.'
        }

        [IO.File]::WriteAllText((Join-Path $Directory 'close.request'), '')
        $closed = Wait-Receipt $process $Directory 2
        Assert-Window $process $closed 'strip' $Edge $Affinity
        if ($closed.hwnd -ne $strip.hwnd) { throw 'The closed panel changed window identity.' }

        [IO.File]::WriteAllText((Join-Path $Directory 'quit.request'), '')
        if (-not $process.WaitForExit(20000) -or $process.ExitCode -ne 0) {
            throw 'The tray quit handler did not exit cleanly.'
        }
        if ($Settings -and [IO.File]::ReadAllText($settingsPath) -cne $Settings) {
            throw 'Rooom altered an existing fixture settings file.'
        }
        if (-not $Settings) {
            $defaults = Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json
            if (-not $defaults.captureExclusion -or $null -ne $defaults.shortcut) {
                throw 'Rooom did not save the capture and shortcut defaults.'
            }
        }
        [pscustomobject]@{ Profile = (Split-Path $Directory -Leaf); Edge = $Edge; Affinity = $Affinity; SameWindow = $true; Exit = 0 }
    } finally {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }
}

function Send-Marker([string]$Directory, [string]$Name) {
    [IO.File]::WriteAllText((Join-Path $Directory $Name), '')
}

function Stop-Skeleton($Process) {
    if (-not $Process.HasExited) { $Process.Kill(); $Process.WaitForExit() }
    $Process.Dispose()
}

function Wait-ExitZero($Process, [string]$Reason) {
    if (-not $Process.WaitForExit(20000) -or $Process.ExitCode -ne 0) {
        throw "$Reason did not complete successfully."
    }
}

function Assert-Health($Process, [string]$Directory) {
    $probe = Start-Skeleton $Directory -Full -Health
    try {
        Wait-ExitZero $probe 'Health probe'
        $health = Get-Content -LiteralPath (Join-Path $Directory 'health.txt') -Raw | ConvertFrom-Json
        if ($health.pid -ne $Process.Id -or $health.session -ne $Process.SessionId) {
            throw 'Health did not return the original resident PID and Windows session ID.'
        }
    } finally {
        Stop-Skeleton $probe
    }
}

function Invoke-FullProfile([string]$Directory) {
    New-Item -ItemType Directory -Path $Directory | Out-Null
    $settingsPath = Join-Path $Directory 'settings.json'
    $settings = '{"edge":"bottom","display":"Fixture display","shortcut":"Ctrl+Alt+Shift+F9","captureExclusion":false}'
    [IO.File]::WriteAllText($settingsPath, $settings)
    $process = Start-Skeleton $Directory -Full
    try {
        $strip = Wait-Receipt $process $Directory 0
        Assert-Window $process $strip 'strip' 'bottom' 0
        if (-not $strip.shortcutRegistered -or $strip.snapshot.shortcutIssue -or
            $strip.snapshot.settings.display -cne 'Fixture display' -or
            $strip.snapshot.settings.shortcut -cne 'Ctrl+Alt+Shift+F9') {
            throw 'The saved settings or real OS shortcut registration could not be read back.'
        }
        Assert-Health $process $Directory

        $foreground = [RooomWindowProof]::GetForegroundWindow().ToInt64()
        Send-Marker $Directory 'pointer-near'
        $panel = Wait-Receipt $process $Directory 1
        Assert-Window $process $panel 'panel' 'bottom' 0
        if ($panel.foreground -ne $foreground -or $panel.foreground -eq $panel.hwnd) {
            throw 'The edge strip activated Rooom during passive reveal.'
        }
        $clock = [Diagnostics.Stopwatch]::StartNew()
        Remove-Item -LiteralPath (Join-Path $Directory 'pointer-near')
        Assert-Window $process (Wait-Receipt $process $Directory 2) 'strip' 'bottom' 0
        if ($clock.Elapsed.TotalSeconds -lt 2.9 -or $clock.Elapsed.TotalSeconds -gt 12) {
            throw "Automatic retraction elapsed $($clock.Elapsed.TotalSeconds) seconds."
        }

        Send-Marker $Directory 'shortcut.request'
        Assert-Window $process (Wait-Receipt $process $Directory 3) 'panel' 'bottom' 0
        Send-Marker $Directory 'pin.request'
        $pinned = Wait-Receipt $process $Directory 4
        if (-not $pinned.snapshot.pinned) { throw 'Pin state was not rendered.' }
        Start-Sleep -Milliseconds 3350
        if (Test-Path -LiteralPath (Join-Path $Directory '5.json')) { throw 'The pinned panel retracted.' }
        Assert-Window $process $pinned 'panel' 'bottom' 0
        Send-Marker $Directory 'unpin.request'
        $unpinned = Wait-Receipt $process $Directory 5
        if ($unpinned.snapshot.pinned) { throw 'The panel did not unpin.' }
        Assert-Window $process (Wait-Receipt $process $Directory 6) 'strip' 'bottom' 0

        $foreground = [RooomWindowProof]::GetForegroundWindow().ToInt64()
        $duplicate = Start-Skeleton $Directory -Full
        try {
            Wait-ExitZero $duplicate 'Duplicate launch'
            $woken = Wait-Receipt $process $Directory 7
            Assert-Window $process $woken 'panel' 'bottom' 0
            if ($woken.hwnd -ne $strip.hwnd -or $duplicate.Id -eq $process.Id) {
                throw 'The duplicate did not wake the original resident window.'
            }
            if ($woken.foreground -ne $foreground -or $woken.foreground -eq $woken.hwnd) {
                throw 'The incoming pipe request acquired focus.'
            }
            Assert-Health $process $Directory
            "D-049 same-session resident PID $($process.Id), duplicate PID $($duplicate.Id) exited 0, session $($process.SessionId)."
        } finally {
            Stop-Skeleton $duplicate
        }

        Send-Marker $Directory 'pointer-near'
        $debug = [RooomWindowProof+FocusDebugScope]::new([uint32]$process.Id)
        try {
            Send-Marker $Directory 'focus.request'
            $limit = [DateTime]::UtcNow.AddSeconds(2)
            while ([RooomWindowProof]::GetForegroundWindow().ToInt64() -ne $woken.hwnd) {
                if ([DateTime]::UtcNow -ge $limit) { throw 'The panel did not acquire native focus on explicit interaction.' }
                Start-Sleep -Milliseconds 40
            }
            Remove-Item -LiteralPath (Join-Path $Directory 'pointer-near')
            Start-Sleep -Milliseconds 3350
            Assert-Window $process $woken 'panel' 'bottom' 0
            if (Test-Path -LiteralPath (Join-Path $Directory '8.json')) { throw 'The focused panel retracted.' }
        } finally {
            $debug.Dispose()
        }
        Send-Marker $Directory 'close.request'
        Assert-Window $process (Wait-Receipt $process $Directory 8) 'strip' 'bottom' 0
        Send-Marker $Directory 'quit.request'
        Wait-ExitZero $process 'Tray quit'
        if ([IO.File]::ReadAllText($settingsPath) -cne $settings) { throw 'The first run altered saved settings.' }
    } finally {
        Stop-Skeleton $process
    }

    $restarted = $Directory
    foreach ($marker in @('open', 'shortcut', 'pin', 'unpin', 'focus', 'close', 'quit')) {
        $file = Join-Path $restarted "$marker.request"
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file }
    }
    foreach ($revision in 0..8) { Remove-Item -LiteralPath (Join-Path $restarted "$revision.json") }
    $updated = $settings.Replace('"captureExclusion":false', '"captureExclusion":true')
    [IO.File]::WriteAllText($settingsPath, $updated)
    Send-Marker $restarted 'conflict'
    if (-not [RooomWindowProof]::RegisterHotKey([IntPtr]::Zero, 27, 7, 0x78)) {
        throw 'Could not reserve the fixture hotkey for the conflict check.'
    }
    try {
        $process = Start-Skeleton $restarted -Full
        try {
            $strip = Wait-Receipt $process $restarted 0
            Assert-Window $process $strip 'strip' 'bottom' 17
            if ($strip.shortcutRegistered -or $strip.snapshot.shortcutIssue -cne 'unavailable' -or
                $strip.snapshot.settings.display -cne 'Fixture display' -or
                $strip.snapshot.settings.shortcut -cne 'Ctrl+Alt+Shift+F9') {
                throw 'Restarted settings or real OS hotkey conflict was not reflected in the snapshot.'
            }
            Send-Marker $restarted 'open.request'
            $panel = Wait-Receipt $process $restarted 1
            Assert-Window $process $panel 'panel' 'bottom' 17
            if (-not $panel.warningVisible) { throw 'The shortcut warning was not rendered.' }
            Send-Marker $restarted 'quit.request'
            Wait-ExitZero $process 'Tray quit after conflict'
            if ([IO.File]::ReadAllText($settingsPath) -cne $updated) { throw 'Restart altered saved settings.' }
        } finally {
            Stop-Skeleton $process
        }
    } finally {
        if (-not [RooomWindowProof]::UnregisterHotKey([IntPtr]::Zero, 27)) {
            throw 'Could not release the temporary hotkey reservation.'
        }
    }
    'Full interactive check passed: passive reveal, timed retraction, pin/focus, registered shortcut handler, OS key conflict, tray fallback, capture affinity and restart.'
}

function Assert-SessionProof([string]$Receipt, [string]$Target) {
    if (-not $Receipt) { throw 'Full needs -SessionProof from scripts/check-pipe-sessions.ps1 on an isolated Windows guest.' }
    cargo rustc --locked -p rooom-platform --release --target $Target --example pipe_fixture -- -C target-feature=+crt-static
    $hash = (Get-FileHash -LiteralPath "target/$Target/release/examples/pipe_fixture.exe" -Algorithm SHA256).Hash
    $proof = Get-Content -LiteralPath $Receipt -Raw | ConvertFrom-Json
    if ($proof.check -cne 'rooom-d049-session-routing-v1' -or $proof.fixtureSha256 -cne $hash -or
        $proof.sameSid -ne $true -or $proof.console.session -eq $proof.peer.session -or
        $proof.console.pid -eq $proof.peer.pid -or $proof.console.pid -le 0 -or $proof.peer.pid -le 0 -or
        $proof.crossSessionRejected -ne $true -or $proof.foreignAccountError -ne 5) {
        throw 'The isolated guest receipt did not prove D-049 with this fixture build.'
    }
    "D-049 isolated Windows sessions $($proof.console.session) and $($proof.peer.session), same SID; cross-session traffic rejected and foreign SID returned ERROR_ACCESS_DENIED."
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    if (-not (Test-Path -LiteralPath dist/index.html)) {
        throw 'Run scripts/check-windows-build.ps1 first to build the frontend.'
    }
    $target = 'x86_64-pc-windows-msvc'
    $config = $env:TAURI_CONFIG
    try {
        $env:TAURI_CONFIG = '{"app":{"security":{"capabilities":["shell","smoke"]}}}'
        cargo build --locked -p rooom-app --target $target --profile smoke --features custom-protocol,first-batch-smoke
    } finally {
        $env:TAURI_CONFIG = $config
    }
    $exe = (Resolve-Path -LiteralPath "target/$target/smoke/rooom.exe").Path
    $project = (Get-Location).Path
    $name = 'rooom-m2-smoke-' + [Guid]::NewGuid().ToString('N')
    $root = Join-Path $project $name
    New-Item -ItemType Directory -Path $root | Out-Null
    try {
        "Suite $Suite"
        if ($Suite -eq 'Full') {
            cargo test --locked -p rooom-platform --target $target
            Invoke-FullProfile (Join-Path $root 'interactive')
            Assert-SessionProof $SessionProof $target
        } else {
            Invoke-CleanProfile (Join-Path $root 'defaults') '' 'right' 17 $false
            $settings = '{"edge":"bottom","display":"Fixture display","shortcut":"Ctrl+Alt+R","captureExclusion":false}'
            Invoke-CleanProfile (Join-Path $root 'saved-bottom-edge') $settings 'bottom' 0 $false
            Invoke-CleanProfile (Join-Path $root 'malformed-settings') '{invalid' 'right' 17 $true
        }
    } finally {
        $fixture = Get-Item -LiteralPath $root
        if ($fixture.FullName -cne $root -or $fixture.Parent.FullName -cne $project -or
            ($fixture.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Unexpected smoke root.' }
        if (@(Get-ChildItem -LiteralPath $root -Recurse -Force -Attributes ReparsePoint).Count) {
            throw "Unexpected link in the smoke root $root."
        }
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            try {
                Remove-Item -LiteralPath $fixture.FullName -Recurse -Force
                break
            } catch [System.IO.IOException] {
                if ($attempt -eq 19) { throw }
                Start-Sleep -Milliseconds 250
            }
        }
        if (Test-Path -LiteralPath $root) { throw "Smoke directory remains at $root." }
    }
} finally {
    Pop-Location
}
