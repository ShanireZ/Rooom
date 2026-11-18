param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][string]$Rooom,
    [ValidateRange(5, 300)][int]$TimeoutSeconds = 90
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$Root = (Resolve-Path -LiteralPath $Root).Path
$Rooom = (Resolve-Path -LiteralPath $Rooom).Path
if ((Split-Path -Leaf $Root) -cne 'claude') {
    throw 'The Claude runner requires a disposable adapter directory named claude.'
}

function Write-Record([string]$Path, $Record, [switch]$Once) {
    $stage = $Path + '.tmp-' + $PID
    [IO.File]::WriteAllText($stage, ($Record | ConvertTo-Json -Depth 8 -Compress))
    try {
        [IO.File]::Move($stage, $Path, -not $Once)
    } finally {
        if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage }
    }
}

$self = Get-Process -Id $PID
$plugin = Join-Path $Root 'plugin'
$checkIns = Join-Path $plugin 'check-ins'
$receiptFolder = Join-Path $plugin 'receipts/session-1'
foreach ($path in @((Join-Path $Root 'sessions'), $checkIns, $receiptFolder)) {
    [IO.Directory]::CreateDirectory($path) | Out-Null
}
$configPath = Join-Path $Root 'client.json'
if (-not (Test-Path -LiteralPath $configPath)) {
    Write-Record $configPath @{
        enabled = $true
        desktopVersion = '5.11492.5'
        engineVersion = '2.1.325'
        processImage = $self.MainModule.FileName
    } -Once
}
$registry = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'registry.json') -Raw | ConvertFrom-Json -AsHashtable
$registry.pid = $PID
$registry.procStart = $self.StartTime.ToFileTimeUtc().ToString([Globalization.CultureInfo]::InvariantCulture)
Write-Record (Join-Path $Root "sessions/$PID.json") $registry -Once

$process = $null
$claimCount = 0
$hookStarted = $false
$elapsed = [Diagnostics.Stopwatch]::StartNew()
$lastCheckIn = -5000
try {
    while ($elapsed.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        if (Test-Path -LiteralPath (Join-Path $Root 'stop-runner')) { exit 0 }
        if ($elapsed.ElapsedMilliseconds - $lastCheckIn -ge 5000) {
            $lastCheckIn = $elapsed.ElapsedMilliseconds
            Write-Record (Join-Path $checkIns 'session-1.json') @{
                sessionId = 'session-1'
                at = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                version = '2.1.325'
                channelVersion = 1
            }
        }
        if (-not $hookStarted -and (Test-Path -LiteralPath (Join-Path $Root 'start-hook'))) {
            $hookStarted = $true
            $process = Start-Process -FilePath $Rooom -ArgumentList '--hook' -WorkingDirectory $Root `
                -WindowStyle Hidden -PassThru `
                -RedirectStandardInput (Join-Path $PSScriptRoot 'permission-request.json') `
                -RedirectStandardOutput (Join-Path $Root 'hook-stdout.json') `
                -RedirectStandardError (Join-Path $Root 'hook-stderr.log')
        }
        if ($null -ne $process -and $process.HasExited) {
            $process.WaitForExit()
            Write-Record (Join-Path $Root 'hook-exit.json') @{ exitCode = $process.ExitCode } -Once
            $process.Dispose()
            $process = $null
        }
        $claimSignal = Join-Path $Root 'start-claim'
        if (Test-Path -LiteralPath $claimSignal) {
            $id = (Get-Content -LiteralPath $claimSignal -Raw).Trim()
            if ($id -cnotmatch '^[a-zA-Z0-9_-]{1,128}$') { throw 'Invalid fixture delivery ID.' }
            $pending = Join-Path $plugin "pending/session-1/$id.json"
            $taken = Join-Path $receiptFolder "$id.json"
            if ((Test-Path -LiteralPath $pending) -and -not (Test-Path -LiteralPath $taken)) {
                $claim = Join-Path $Root "$id.claim"
                [IO.File]::Move($pending, $claim)
                try {
                    $message = Get-Content -LiteralPath $claim -Raw | ConvertFrom-Json
                    if ($message.id -cne $id -or $message.sessionId -cne 'session-1' -or
                        [string]::IsNullOrWhiteSpace($message.text) -or
                        $message.expiresAt -le [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) {
                        throw 'Unexpected fixture delivery.'
                    }
                } finally {
                    Remove-Item -LiteralPath $claim
                }
                Write-Record $taken @{ id = $id; sessionId = 'session-1'; state = 'taken' } -Once
                $claimCount++
                Write-Record (Join-Path $Root 'plugin-claim.json') @{ id = $id; sessionId = 'session-1'; count = $claimCount }
            }
        }
        $startSignal = Join-Path $Root 'start-turn'
        if (Test-Path -LiteralPath $startSignal) {
            $id = (Get-Content -LiteralPath $startSignal -Raw).Trim()
            if ($id -cnotmatch '^[a-zA-Z0-9_-]{1,128}$') { throw 'Invalid fixture turn ID.' }
            $current = Join-Path $receiptFolder "$id.json"
            if (Test-Path -LiteralPath $current) {
                $receipt = Get-Content -LiteralPath $current -Raw | ConvertFrom-Json
                if ($receipt.state -ceq 'taken') {
                    Write-Record $current @{ id = $id; sessionId = 'session-1'; state = 'started' }
                }
            }
        }
        Start-Sleep -Milliseconds 75
    }
    throw 'The Claude fixture runner timed out.'
} finally {
    if ($null -ne $process) {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
    }
    $self.Dispose()
}
