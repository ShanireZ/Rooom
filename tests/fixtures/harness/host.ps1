param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][string]$ExpectedPrompt)

$ErrorActionPreference = 'Stop'
function Put([string]$Path, $Value) {
    $temp = "$Path.tmp"
    [IO.File]::WriteAllText($temp, (ConvertTo-Json -InputObject $Value -Depth 12 -Compress))
    $wait = [Diagnostics.Stopwatch]::StartNew()
    $attempt = 0
    try {
        while ($true) {
            try {
                [IO.File]::Move($temp, $Path, $true)
                return
            } catch {
                $code = $_.Exception.GetBaseException().HResult -band 0xffff
                if ($code -notin @(5, 32, 33) -or $wait.ElapsedMilliseconds -ge 2000) { throw }
                $attempt++
                if ($attempt -eq 1) {
                    [Console]::Error.WriteLine((ConvertTo-Json -InputObject @{
                        event = 'publication-retry'; file = [IO.Path]::GetFileName($Path); win32 = $code
                    } -Compress))
                }
                Start-Sleep -Milliseconds 20
            }
        }
    } finally {
        if ([IO.File]::Exists($temp)) { [IO.File]::Delete($temp) }
    }
}
function Get-Json([string]$Path) { Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json }

$childDirs = @('events', 'prompts', 'answers', 'decided', 'receipts')
foreach ($name in $childDirs) { New-Item -Path (Join-Path $Root $name) -ItemType Directory | Out-Null }
$checkIn = Get-Json (Join-Path $PSScriptRoot 'check-in.json')
$approval = Get-Json (Join-Path $PSScriptRoot 'approval.json')
$controller = @{ queues = [System.Collections.Generic.HashSet[string]]::new(); starts = [System.Collections.Generic.HashSet[string]]::new() }
function Submit-ControllerPrompt($Request) {
    if ($Request.sessionId -cne 'session-1' -or $Request.mode -cne 'queue' -or
        $Request.content.Count -ne 1 -or $Request.content[0].type -cne 'text' -or
        $Request.content[0].text -cne $ExpectedPrompt -or -not $controller.queues.Add($Request.requestId)) {
        throw 'Harness controller rejected synthetic prompt identity, mode or payload.'
    }
    @{ accepted = $true }
}
Put (Join-Path $Root 'settings.json') @{ enabled = $true }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'message.json') -Destination (Join-Path $Root 'events/message.json')
$clock = [Diagnostics.Stopwatch]::StartNew()
$published = $false
while ($clock.Elapsed.TotalSeconds -lt 90 -and -not (Test-Path -LiteralPath (Join-Path $Root 'stop-host'))) {
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $checkIn.at = $now
    if (-not (Test-Path -LiteralPath (Join-Path $Root 'stop-check-ins'))) {
        Put (Join-Path $Root 'check-in.json') $checkIn
    } elseif (-not (Test-Path -LiteralPath (Join-Path $Root 'check-ins-stopped.json'))) {
        Put (Join-Path $Root 'check-ins-stopped.json') @{ stopped = $true }
    }
    if (-not $published -and (Test-Path -LiteralPath (Join-Path $Root 'start-approval'))) {
        $approval.at = $now
        Put (Join-Path $Root 'events/approval.json') $approval
        $published = $true
    }
    $answer = Join-Path $Root 'answers/request-1.json'
    if (Test-Path -LiteralPath $answer) {
        $result = Get-Json $answer
        if ($result.sessionId -cne 'session-1' -or $result.callId -cne 'request-1' -or
            $result.outcome -cnotin @('allowed-once','denied') -or (Test-Path -LiteralPath (Join-Path $Root 'decided/request-1.json'))) {
            throw 'Harness received an invalid or repeated approval decision.'
        }
        Put (Join-Path $Root 'decided/request-1.json') $result
        Remove-Item -LiteralPath $answer
    }
    if (Test-Path -LiteralPath (Join-Path $Root 'claim-prompts')) {
        foreach ($file in Get-ChildItem -LiteralPath (Join-Path $Root 'prompts') -Filter '*.json') {
            $claimed = $file.FullName + '.taken'
            try { [IO.File]::Move($file.FullName, $claimed) } catch [IO.FileNotFoundException] { continue }
            try { $prompt = Get-Json $claimed } finally { Remove-Item -LiteralPath $claimed }
            if ($prompt.expiresAt -le $now) { throw 'Expired prompt reached the controller.' }
            if ((Submit-ControllerPrompt $prompt).accepted) {
                Put (Join-Path $Root 'controller-accepted.json') @{
                    requestId = $prompt.requestId; sessionId = $prompt.sessionId; count = $controller.queues.Count
                }
                Put (Join-Path $Root "receipts/$($prompt.requestId).json") @{
                    requestId = $prompt.requestId; sessionId = $prompt.sessionId; stage = 'accepted'
                }
            }
        }
    }
    $start = Join-Path $Root 'start-turn'
    if (Test-Path -LiteralPath $start) {
        $id = [IO.File]::ReadAllText($start).Trim()
        if ($controller.queues.Contains($id) -and $controller.starts.Add($id)) {
            Put (Join-Path $Root "receipts/$id.json") @{ requestId = $id; sessionId = 'session-1'; stage = 'started' }
        }
    }
    Start-Sleep -Milliseconds 100
}
