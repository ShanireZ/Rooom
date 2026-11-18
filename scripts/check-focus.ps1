param([ValidateRange(1, 30)][int]$Repetitions = 3)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $IsWindows -or (Get-Process -Id $PID).SessionId -eq 0) {
    throw 'Run the focus regression in a Windows desktop session.'
}
. (Join-Path $PSScriptRoot 'focus-proof.ps1')
Add-Type -Path (Join-Path $PSScriptRoot '../tests/fixtures/focus/ControlWindow.cs')

function Require($Value, [string]$Reason) { if (-not $Value) { throw $Reason } }
function Marker([string]$Folder, [string]$Name) { [IO.File]::WriteAllText((Join-Path $Folder "$Name.request"), '') }
function Wait-For([string]$Reason, $Process, [scriptblock]$Condition) {
    $until = [DateTime]::UtcNow.AddSeconds(40)
    do {
        if ($Process.HasExited) { throw "Rooom exited while waiting for $Reason." }
        $value = & $Condition
        if ($value) { return $value }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $until)
    throw "Timed out waiting for $Reason."
}
function Receipt($Process, [string]$Folder, [int]$Revision) {
    Wait-For "WebView receipt $Revision" $Process {
        $path = Join-Path $Folder "$Revision.json"
        if (Test-Path -LiteralPath $path) {
            try { Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -AsHashtable }
            catch [IO.IOException] { }
        }
    }
}
function Assert-Rejected($Frame) {
    $rejected = $false
    try { Assert-PassiveFocus $Frame } catch { $rejected = $true }
    Require $rejected 'The focus check accepted an actual activation.'
}
function Run-Case([string]$Name, [bool]$ActivateRooom) {
    $profile = Join-Path $root $Name
    New-Item -ItemType Directory -Path $profile | Out-Null
    $first = $null
    $second = $null
    $resident = $null
    try {
        $first = [RooomFocusControl]::new()
        Require ($first.Activate()) 'The first owned control did not become foreground.'
        $resident = Start-Process -FilePath $exe -WorkingDirectory $profile -WindowStyle Hidden -Environment @{
            ROOOM_SMOKE_ROOT = $profile
            ROOOM_SMOKE_ID = (Split-Path $root -Leaf) + '-' + $Name
            ROOOM_SMOKE_FULL = if ($ActivateRooom) { '1' } else { $null }
        } -RedirectStandardOutput "$profile/resident.out.log" -RedirectStandardError "$profile/resident.err.log" -PassThru
        $strip = Receipt $resident $profile 0
        Assert-PassiveFocus $strip
        $second = [RooomFocusControl]::new()
        Require ($second.Activate()) 'The second owned control did not become foreground.'
        Marker $profile $(if ($ActivateRooom) { 'shortcut' } else { 'open' })
        $panel = Receipt $resident $profile 1
        Require ($strip.foreground -ne $panel.foreground -and $panel.foreground -ne $panel.hwnd) 'The outside-only foreground transition was not exercised.'
        Assert-PassiveFocus $panel

        if ($ActivateRooom) {
            Marker $profile 'pin'
            Receipt $resident $profile 2 | Out-Null
            Marker $profile 'unpin'
            Receipt $resident $profile 3 | Out-Null
            Require ($second.Grant([uint32]$resident.Id)) 'The owned control could not grant foreground to the test copy.'
            Marker $profile 'focus'
            Wait-For 'actual Rooom activation' $resident {
                [RooomFocusControl]::GetForegroundWindow().ToInt64() -eq $panel.hwnd
            } | Out-Null
            Require ($second.Activate()) 'The owned control did not recover foreground.'
            Marker $profile 'close'
            $closed = Receipt $resident $profile 4
            Require ($closed.foreground -eq $second.Hwnd) 'The transient activation control did not end outside Rooom.'
            Require ($closed.focusProof.focusableObservations -gt 0) 'The transient activation has no native focus evidence.'
            Assert-Rejected $closed
        } else {
            Marker $profile 'close'
            Receipt $resident $profile 2 | Out-Null
        }
        Marker $profile 'quit'
        Require ($resident.WaitForExit(15000) -and $resident.ExitCode -eq 0) 'The focus fixture did not exit cleanly.'
    } finally {
        if ($resident) {
            if (-not $resident.HasExited) { $resident.Kill(); $resident.WaitForExit() }
            $resident.Dispose()
        }
        if ($second) { $second.Dispose() }
        if ($first) { $first.Dispose() }
    }
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $project = (Get-Location).Path
    $target = 'x86_64-pc-windows-msvc'
    $config = $env:TAURI_CONFIG
    try {
        $env:TAURI_CONFIG = '{"app":{"security":{"capabilities":["shell","smoke"]}}}'
        cargo build --locked -p rooom-app --target $target --profile smoke --features custom-protocol,first-batch-smoke
    } finally {
        $env:TAURI_CONFIG = $config
    }
    $root = Join-Path $project ('rooom-m3-focus-' + (Get-Date -Format yyyyMMdd) + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root | Out-Null
    try {
        # Negative controls run a disposable executable copy, with its own profiles.
        $exe = Join-Path $root 'rooom-focus-fixture.exe'
        Copy-Item -LiteralPath "target/$target/smoke/rooom.exe" -Destination $exe
        for ($run = 1; $run -le $Repetitions; $run++) {
            Run-Case "passive-$run" $false
            Run-Case "activation-$run" $true
        }
        "Focus PASS: $Repetitions outside-only switches accepted and $Repetitions actual transient activations rejected."
    } finally {
        $directory = Get-Item -LiteralPath $root
        if ($directory.FullName -cne $root -or $directory.Parent.FullName -cne $project -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            @(Get-ChildItem -LiteralPath $root -Recurse -Force -Attributes ReparsePoint).Count) {
            throw 'Unexpected focus-fixture cleanup root.'
        }
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            try { Remove-Item -LiteralPath $directory.FullName -Recurse -Force; break }
            catch [IO.IOException] { if ($attempt -eq 19) { throw }; Start-Sleep -Milliseconds 250 }
        }
        if (Test-Path -LiteralPath $root) { throw 'The disposable focus directory was not removed.' }
    }
} finally {
    Pop-Location
}
