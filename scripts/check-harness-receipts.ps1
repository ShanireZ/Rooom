param([ValidateRange(1, 20)][int]$Repetitions = 3)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $IsWindows) { throw 'The Harness receipt fixture requires Windows file sharing.' }

function Require($Value, [string]$Reason) { if (-not $Value) { throw $Reason } }
function Json([string]$Path) { Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json }
function Put([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Compress -Depth 6))
}
function Wait-For([string]$Description, $HostProcess, [scriptblock]$Probe) {
    $until = [DateTime]::UtcNow.AddSeconds(8)
    do {
        if ($HostProcess.HasExited) { throw "The fixture Host exited while waiting for $Description." }
        $value = & $Probe
        if ($value) { return $value }
        Start-Sleep -Milliseconds 20
    } while ([DateTime]::UtcNow -lt $until)
    throw "Timed out waiting for $Description."
}
function Wait-ForRetry([string]$Folder, $HostProcess, [string]$Path) {
    $leaf = [IO.Path]::GetFileName($Path)
    Wait-For 'a real publication sharing failure' $HostProcess {
        $lines = Get-Content -LiteralPath "$Folder/host.err.log"
        @($lines | Where-Object { $_ -match '"publication-retry"' -and $_ -match [regex]::Escape($leaf) }).Count -gt 0
    } | Out-Null
}
function Run-Case([string]$Name, [string]$LockKind) {
    $folder = Join-Path $root $Name
    New-Item -ItemType Directory -Path $folder | Out-Null
    $child = $null
    $reader = $null
    try {
        $arguments = @(
            '-NoProfile', '-File', $hostScript, '-Root', $folder, '-ExpectedPrompt', $marker
        ) | ForEach-Object { '"' + $_ + '"' }
        $child = Start-Process -FilePath (Get-Process -Id $PID).Path -WindowStyle Hidden -ArgumentList ($arguments -join ' ') -RedirectStandardOutput "$folder/host.out.log" -RedirectStandardError "$folder/host.err.log" -PassThru
        Wait-For 'the first check-in' $child { Test-Path -LiteralPath "$folder/check-in.json" } | Out-Null
        $receipt = "$folder/receipts/receipt-fixture.json"
        $locked = "$folder/check-in.json"
        if ($LockKind -eq 'receipt') {
            $locked = $receipt
            Put $locked @{ requestId = 'previous-fixture'; sessionId = 'session-1'; stage = 'previous-synthetic' }
        }
        if ($LockKind -ne 'none') {
            $reader = [IO.File]::Open($locked, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
            $before = [IO.File]::ReadAllBytes($locked)
        }
        if ($LockKind -in @('check-in', 'permanent')) { Wait-ForRetry $folder $child $locked }
        Put "$folder/prompts/receipt-fixture.json" @{
            requestId = 'receipt-fixture'; sessionId = 'session-1'; mode = 'queue'
            content = @(@{ type = 'text'; text = $marker })
            expiresAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() + 60000
        }
        [IO.File]::WriteAllText("$folder/claim-prompts", '')
        if ($LockKind -eq 'permanent') {
            $elapsed = [Diagnostics.Stopwatch]::StartNew()
            Require ($child.WaitForExit(6000) -and $child.ExitCode -ne 0) 'A persistent lock was not bounded.'
            Require ($elapsed.ElapsedMilliseconds -lt 6000) 'Persistent-lock failure exceeded its bound.'
            Require ([Linq.Enumerable]::SequenceEqual([byte[]]$before, [byte[]][IO.File]::ReadAllBytes($locked))) 'A failed publication changed the previous check-in.'
            Require (-not (Test-Path -LiteralPath "$locked.tmp")) 'A failed publication left its staging file.'
            Require (Test-Path -LiteralPath "$folder/prompts/receipt-fixture.json") 'A prompt was claimed while check-in publication was blocked.'
            Require (-not (Test-Path -LiteralPath $receipt)) 'A blocked Host invented a receipt.'
        } else {
            if ($reader) {
                Wait-ForRetry $folder $child $locked
                Require ([Linq.Enumerable]::SequenceEqual([byte[]]$before, [byte[]][IO.File]::ReadAllBytes($locked))) 'The blocked publication changed the previous bytes.'
                $reader.Dispose()
                $reader = $null
            }
            Wait-For 'the accepted receipt' $child {
                if (Test-Path -LiteralPath $receipt) { (Json $receipt).stage -ceq 'accepted' }
            } | Out-Null
            $admission = Json "$folder/controller-accepted.json"
            Require ($admission.requestId -ceq 'receipt-fixture' -and $admission.count -eq 1) 'The controller did not accept exactly one request.'
            Require (-not (Test-Path -LiteralPath "$folder/prompts/receipt-fixture.json")) 'The accepted prompt remained published.'
            [IO.File]::WriteAllText("$folder/start-turn", 'receipt-fixture')
            Wait-For 'the started receipt' $child { (Json $receipt).stage -ceq 'started' } | Out-Null
            Require ((Json "$folder/controller-accepted.json").count -eq 1) 'Recovery resubmitted the controller request.'
            [IO.File]::WriteAllText("$folder/stop-host", '')
            Require ($child.WaitForExit(5000) -and $child.ExitCode -eq 0) 'The recovered Host did not exit cleanly.'
        }
    } finally {
        if ($reader) { $reader.Dispose() }
        if ($child) {
            if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
            $child.Dispose()
        }
    }
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $project = (Get-Location).Path
    $hostScript = Join-Path $project 'tests/fixtures/harness/host.ps1'
    $marker = 'ROOOM_HARNESS_RECEIPT_FIXTURE'
    $root = Join-Path $project ('rooom-m3-harness-receipts-' + (Get-Date -Format yyyyMMdd) + '-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root | Out-Null
    try {
        for ($run = 1; $run -le $Repetitions; $run++) {
            foreach ($kind in @('none', 'check-in', 'receipt', 'permanent')) {
                Run-Case "$kind-$run" $kind
            }
        }
        "Harness receipts PASS: $Repetitions healthy, check-in-lock recovery, receipt-lock recovery and bounded permanent-lock cases."
    } finally {
        $directory = Get-Item -LiteralPath $root
        if ($directory.FullName -cne $root -or $directory.Parent.FullName -cne $project -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            @(Get-ChildItem -LiteralPath $root -Recurse -Force -Attributes ReparsePoint).Count) {
            throw 'Unexpected Harness receipt cleanup root.'
        }
        Remove-Item -LiteralPath $directory.FullName -Recurse -Force
    }
} finally { Pop-Location }
