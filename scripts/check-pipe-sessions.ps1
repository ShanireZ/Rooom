param(
    [Parameter(Mandatory)][string]$Fixture,
    [Parameter(Mandatory)][string]$Proof,
    [switch]$Isolated
)

$ErrorActionPreference = 'Stop'
if (-not $Isolated -or $env:OS -ne 'Windows_NT' -or (Get-Process -Id $PID).SessionId -eq 0) {
    throw 'Run this gate from the console of a disposable isolated Windows installation with -Isolated.'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)) {
    throw 'The isolated session gate needs an elevated console.'
}

$exe = (Resolve-Path -LiteralPath $Fixture).Path
$tag = [Guid]::NewGuid().ToString('N').Substring(0, 10)
$root = Join-Path (Split-Path $exe -Parent) "rooom-pipe-$tag"
$process = $null
$taskNames = @()
$taskService = New-Object -ComObject 'Schedule.Service'
$taskService.Connect()
$taskFolder = $taskService.GetFolder('\')

function Start-TaskFixture($Name, $Path, $Arguments, $Owner, $Password, $LogonType, $Highest) {
    $task = $taskService.NewTask(0)
    $task.Settings.ExecutionTimeLimit = 'PT4M'
    $task.Settings.DisallowStartIfOnBatteries = $false
    $task.Settings.StopIfGoingOnBatteries = $false
    $task.Principal.UserId = $Owner
    $task.Principal.LogonType = $LogonType
    $task.Principal.RunLevel = [int]$Highest
    $action = $task.Actions.Create(0)
    $action.Path = $Path
    $action.Arguments = $Arguments
    $action.WorkingDirectory = Split-Path $Path -Parent
    $registered = $taskFolder.RegisterTaskDefinition($Name, $task, 2, $Owner, $Password, $LogonType, $null)
    $null = $registered.Run($null)
}

function Read-Fixture($Leaf) {
    $path = Join-Path $root $Leaf
    $limit = [DateTime]::UtcNow.AddSeconds(85)
    while ([DateTime]::UtcNow -lt $limit) {
        try {
            if (Test-Path -LiteralPath $path) { return [IO.File]::ReadAllText($path) }
        } catch [IO.IOException] {}
        if ($process.HasExited) {
            $detail = Get-Content -LiteralPath (Join-Path $root 'console.err') -Raw
            throw "Console fixture exited with code $($process.ExitCode) before $Leaf. $detail"
        }
        Start-Sleep -Milliseconds 120
    }
    $files = (Get-ChildItem -File -LiteralPath $root | Select-Object -ExpandProperty Name) -join ', '
    $state = foreach ($name in $taskNames) {
        $registered = $taskFolder.GetTask($name)
        "$($registered.State)/$($registered.LastTaskResult)"
    }
    throw "Timed out waiting for $Leaf. Files $files. Task state/result $state."
}

try {
    New-Item -ItemType Directory -Path $root | Out-Null
    $process = Start-Process -FilePath $exe -ArgumentList "resident `"$root`" console" `
        -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $root 'console.err')
    $peerTask = "RooomPipePeer-$tag"
    $taskNames += $peerTask
    Start-TaskFixture $peerTask $exe "resident `"$root`" peer" $identity.Name $null 2 $true
    $c = (Read-Fixture 'console.result').Split(',')
    $p = (Read-Fixture 'peer.result').Split(',')
    $consoleName = Read-Fixture 'console.ready'
    $peerName = Read-Fixture 'peer.ready'
    $scopePrefix = '\\.\pipe\Rooom-' + $identity.User.Value + '-'
    $peer = Get-Process -Id ([int]$p[0]) -ErrorAction Stop
    if ([int]$c[0] -ne $process.Id -or [int]$c[1] -ne $process.SessionId -or [int]$p[1] -ne $peer.SessionId -or
        [int]$p[0] -eq [int]$c[0] -or [int]$c[1] -eq [int]$p[1] -or
        $consoleName -cne ($scopePrefix + $c[1]) -or $peerName -cne ($scopePrefix + $p[1])) {
        throw 'The simultaneous kernel endpoints did not resolve to the same SID and distinct session/PID pairs.'
    }

    $deniedTask = "RooomPipeDenied-$tag"
    $taskNames += $deniedTask
    # LocalSystem has a different SID. A normal CreateFile from its task
    # must still be denied by the resident's owner-only ACL.
    Start-TaskFixture $deniedTask $exe "denied `"$consoleName`" `"$root\error.txt`"" 'NT AUTHORITY\SYSTEM' $null 5 $false
    if ((Read-Fixture 'error.txt') -cne '5') { throw 'The foreign SID did not receive ERROR_ACCESS_DENIED.' }

    [ordered]@{
        check = 'rooom-d049-session-routing-v1'
        fixtureSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
        sameSid = $true
        console = @{ pid = [int]$c[0]; session = [int]$c[1] }
        peer = @{ pid = [int]$p[0]; session = [int]$p[1] }
        crossSessionRejected = $true
        foreignAccountError = 5
    } | ConvertTo-Json -Depth 3 -Compress | Set-Content -LiteralPath $Proof -Encoding ASCII
} finally {
    if ($process -and -not $process.HasExited) {
        [IO.File]::WriteAllText((Join-Path $root 'release'), '')
        if (-not $process.WaitForExit(5000)) { $process.Kill(); $process.WaitForExit() }
    }
    foreach ($name in $taskNames) {
        try {
            $task = $taskFolder.GetTask($name)
            if ($task.State -eq 4) { $task.Stop(0) }
            $taskFolder.DeleteTask($name, 0)
        } catch {
            if ($_.Exception.HResult -ne -2147024894) { throw }
        }
    }
    if (Test-Path -LiteralPath $root) {
        $directory = Get-Item -LiteralPath $root
        if ($directory.FullName -cne $root -or $directory.Parent.FullName -cne (Split-Path $exe -Parent) -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            @(Get-ChildItem -LiteralPath $root -Recurse -Force -Attributes ReparsePoint).Count) {
            throw 'Unexpected isolated fixture root.'
        }
        Remove-Item -LiteralPath $directory.FullName -Recurse -Force
    }
}
