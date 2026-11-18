param([ValidateSet('Fixtures')][string]$Suite = 'Fixtures')

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
. (Join-Path $PSScriptRoot 'focus-proof.ps1')
if (-not $IsWindows -or (Get-Process -Id $PID).SessionId -eq 0) {
    throw 'Run the fixture gate in a Windows desktop session.'
}
function Require($Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Put([string]$Path, $Value) {
    [IO.File]::WriteAllText("$Path.tmp", (ConvertTo-Json -InputObject $Value -Depth 16 -Compress))
    [IO.File]::Move("$Path.tmp", $Path, $true)
}
function Json([string]$Path) { Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable }
function Marker([string]$Path, [string]$Text = '') { [IO.File]::WriteAllText($Path, $Text) }
function Wait-For([string]$Description, [scriptblock]$Probe, $Resident = $null) {
    $limit = [DateTime]::UtcNow.AddSeconds(35)
    do {
        if ($null -ne $Resident -and $Resident.HasExited) {
            throw "Resident exited ($($Resident.ExitCode)) while waiting for $Description."
        }
        $value = & $Probe
        if ($value) { return $value }
        Start-Sleep -Milliseconds 90
    } while ([DateTime]::UtcNow -lt $limit)
    throw "Timed out waiting for $Description."
}
function View([string]$Profile) {
    $file = Get-ChildItem -LiteralPath $Profile -File -Filter '*.json' |
        Where-Object { $_.BaseName -match '^\d+$' } |
        Sort-Object { [int]$_.BaseName } -Descending | Select-Object -First 1
    if ($file) { Json $file.FullName }
}
function Wait-View([string]$Profile, $Resident, [string]$Description, [scriptblock]$Condition) {
    Wait-For $Description {
        $frame = View $Profile
        if ($frame -and (& $Condition $frame)) { $frame }
    } $Resident
}
function Private-Directory([string]$Path) {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $security = [System.Security.AccessControl.DirectorySecurity]::new()
    $security.SetOwner($sid)
    $security.SetAccessRuleProtection($true, $false)
    $security.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
        $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'
    ))
    $directory = [IO.DirectoryInfo]::new($Path)
    [IO.FileSystemAclExtensions]::Create($directory, $security)
}
function New-Profile([string]$Path, [string[]]$Clients) {
    Private-Directory $Path
    Private-Directory (Join-Path $Path 'adapters')
    foreach ($client in $Clients) { Private-Directory (Join-Path $Path "adapters/$client") }
}
function Start-Fixture([string]$Profile, [string]$Name, [string]$File, [string[]]$Arguments, [int]$TimeoutMs = 25000) {
    $options = @{
        FilePath = $File
        WorkingDirectory = $Profile
        Environment = @{
            ROOOM_SMOKE_ROOT = $Profile
            ROOOM_SMOKE_ID = (Split-Path $root -Leaf) + '-' + (Split-Path $Profile -Leaf)
            ROOOM_SMOKE_FULL = $null
            ROOOM_FIXTURE_HOOK_TIMEOUT_MS = [string]$TimeoutMs
        }
        WindowStyle = 'Hidden'
        RedirectStandardOutput = Join-Path $Profile "$Name.out.log"
        RedirectStandardError = Join-Path $Profile "$Name.err.log"
        PassThru = $true
    }
    if ($Arguments.Count) {
        $options.ArgumentList = ($Arguments | ForEach-Object {
            if ($_ -match '["\r\n]') { throw 'Unexpected fixture argument character.' }
            '"' + $_ + '"'
        }) -join ' '
    }
    $proc = Start-Process @options
    $processes.Add($proc)
    $proc
}
function Start-Claude([string]$Profile, [int]$TimeoutMs = 25000) {
    Start-Fixture $Profile 'claude' $pwsh @(
        '-NoProfile', '-File', (Join-Path $fixtures 'claude/run-plugin-and-hook.ps1'),
        '-Root', (Join-Path $Profile 'adapters/claude'), '-Rooom', $exe, '-TimeoutSeconds', '150'
    ) $TimeoutMs
}
function Stop-Resident([string]$Profile, $Resident) {
    Marker (Join-Path $Profile 'quit.request')
    Require ($Resident.WaitForExit(15000) -and $Resident.ExitCode -eq 0) 'Tray quit did not cleanly stop the fixture resident.'
}
function Act([string]$Profile, $Action) { Put (Join-Path $Profile 'view-action.json') $Action }
function Action-Receipt([string]$Profile, $Resident, [string]$Client, [string]$Stage) {
    Wait-View $Profile $Resident "$Client $Stage receipt" {
        param($frame)
        $matching = @($frame.inboxView.deliveries | Where-Object { $_ -and $_[0] -ceq "$Client/session-1" -and $_[2] -ceq $Stage })
        $matching.Count -eq 1
    }
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $project = (Get-Location).Path
    $fixtures = Join-Path $project 'tests/fixtures'
    $pwsh = (Get-Process -Id $PID).Path
    $target = 'x86_64-pc-windows-msvc'
    Require (Test-Path -LiteralPath 'dist/index.html') 'Run the Windows build gate to build the frontend first.'
    cargo test --locked -p rooom-platform --features adapter-fixtures --target $target
    $config = $env:TAURI_CONFIG
    try {
        $env:TAURI_CONFIG = '{"app":{"security":{"capabilities":["shell","smoke"]}}}'
        cargo build --locked -p rooom-app --target $target --profile smoke --features custom-protocol,first-batch-adapters
    } finally {
        $env:TAURI_CONFIG = $config
    }
    $exe = (Resolve-Path -LiteralPath "target/$target/smoke/rooom.exe").Path
    $root = Join-Path $project ("rooom-m3-fixtures-" + (Get-Date -Format yyyyMMdd) + '-' + [Guid]::NewGuid().ToString('N'))
    Private-Directory $root
    $processes = [Collections.Generic.List[Diagnostics.Process]]::new()
    $marker = 'ROOOM_M3_SYNTHETIC_MESSAGE'
    try {
        $profile = Join-Path $root 'inbox'
        $clients = @('claude', 'codex', 'antigravity', 'harness')
        New-Profile $profile $clients
        $claude = Join-Path $profile 'adapters/claude'
        $codex = Join-Path $profile 'adapters/codex'
        $ag = Join-Path $profile 'adapters/antigravity'
        $dsh = Join-Path $profile 'adapters/harness'
        foreach ($file in Get-ChildItem -LiteralPath (Join-Path $fixtures 'codex') -File) {
            Copy-Item -LiteralPath $file.FullName -Destination $codex
        }
        $codexHistory = "$codex/thread.jsonl"
        $codexHistoryHash = (Get-FileHash -LiteralPath $codexHistory).Hash
        Marker (Join-Path $codex 'expected-prompt.txt') $marker
        Put "$ag/trajectory.json" @()
        $agHistoryDir = "$ag/brain/session-1/.system_generated/logs"
        New-Item -ItemType Directory -Path $agHistoryDir | Out-Null
        $agHistory = "$agHistoryDir/transcript.jsonl"
        Copy-Item -LiteralPath (Join-Path $fixtures 'antigravity/transcript.jsonl') -Destination $agHistory
        $agHistoryHash = (Get-FileHash -LiteralPath $agHistory).Hash
        $claudeHistoryDir = "$claude/projects/fixture-project"
        New-Item -ItemType Directory -Path $claudeHistoryDir | Out-Null
        $claudeHistory = "$claudeHistoryDir/session-1.jsonl"
        Copy-Item -LiteralPath (Join-Path $fixtures 'claude/history.jsonl') -Destination $claudeHistory
        $claudeHistoryHash = (Get-FileHash -LiteralPath $claudeHistory).Hash
        $claudeHost = Start-Claude $profile
        $agHost = Start-Fixture $profile 'antigravity' $pwsh @(
            '-NoProfile', '-File', (Join-Path $fixtures 'antigravity/service.ps1'), '-Root', $ag, '-Mode', 'native2322'
        )
        $dshHost = Start-Fixture $profile 'harness' $pwsh @(
            '-NoProfile', '-File', (Join-Path $fixtures 'harness/host.ps1'), '-Root', $dsh, '-ExpectedPrompt', $marker
        )
        foreach ($path in @("$claude/client.json", "$ag/hub.json", "$dsh/check-in.json")) {
            Wait-For 'client fixture initialization' { Test-Path -LiteralPath $path } | Out-Null
        }
        $settings = @("$claude/client.json", "$codex/settings.json", "$codex/owner.json", "$ag/hub.json", "$dsh/settings.json")
        $hashes = @($settings | ForEach-Object { (Get-FileHash -LiteralPath $_).Hash })
        $secret = (Json "$ag/hub.json").csrfToken
        $resident = Start-Fixture $profile 'resident' $exe @()
        Marker "$claude/start-hook"
        Marker "$dsh/start-approval"
        $initial = Wait-View $profile $resident 'four session rows and four request cards' {
            param($frame)
            $frame.inboxView.sessions.Count -eq 4 -and $frame.inboxView.requests.Count -eq 4
        }
        Assert-PassiveFocus $initial
        foreach ($client in $clients) {
            Require ($initial.inboxView.sessions -ccontains "$client/session-1") "Missing $client session."
            Require (@($initial.inboxView.requests | Where-Object { $_ -and $_[0] -ceq "$client/session-1" -and $_[1] -ceq 'request-1' }).Count -eq 1) "Missing $client request."
        }
        $workspaceCards = @($initial.inboxView.requestWorkspaceCards | Where-Object { $_ })
        Require ($workspaceCards.Count -eq 4 -and ($clients | Where-Object {
            $workspaceCards -cnotcontains "$_/session-1"
        }).Count -eq 0) 'Workspace labels did not render on all four cards with workspace evidence.'
        Require (@($initial.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'harness/session-1' -and $_[1] -ceq 'harness-message-1' }).Count -eq 1) 'Harness live plugin timeline was not rendered.'
        $claudeTimeline = @($initial.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'claude/session-1' } | ForEach-Object { $_[1] })
        Require (($claudeTimeline -join ',') -ceq 'history-prompt-1,history-reply-1') 'Claude transcript messages did not render in native order.'
        $agTimeline = @($initial.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'antigravity/session-1' } | ForEach-Object { $_[1] })
        Require (($agTimeline -join ',') -ceq 'step-1,step-3') 'Antigravity transcript messages did not render in native order.'
        $codexTimeline = @($initial.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'codex/session-1' } | ForEach-Object { $_[1] })
        Require (($codexTimeline -join ',') -ceq 'input:item-prompt,item-intro') 'Codex did not render one prompt followed by its reply.'

        for ($i = 0; $i -lt $clients.Count; $i++) {
            $client = $clients[$i]
            $decision = if ($client -in @('claude', 'harness')) { 'allow' } else { 'deny' }
            Act $profile @{ target = "$client/session-1"; request = 'request-1'; decision = $decision }
            Wait-View $profile $resident "$client answer on WebView" {
                param($frame)
                $frame.inboxView.sessions.Count -eq 4 -and $frame.inboxView.requests.Count -eq (3 - $i) -and
                    @($frame.inboxView.requests | Where-Object { $_ -and $_[0] -ceq "$client/session-1" }).Count -eq 0
            } | Out-Null
        }
        Wait-For 'hook return' { Test-Path -LiteralPath "$claude/hook-exit.json" } $resident | Out-Null
        $hook = Json "$claude/hook-stdout.json"
        Require ($hook.hookSpecificOutput.decision.behavior -ceq 'allow' -and (Json "$claude/hook-exit.json").exitCode -eq 0) 'Claude hook did not receive the explicit owner decision.'
        $codexApproval = Json "$codex/approval-receipt.json"
        Require ($codexApproval.requestId -ceq 'request-1' -and $codexApproval.conversationId -ceq 'session-1' -and
            $codexApproval.result.result.ok -and $codexApproval.result.result.decision -ceq 'decline') 'Codex fixture did not acknowledge the explicit denial.'
        Require ((Json "$dsh/decided/request-1.json").outcome -ceq 'allowed-once') 'Harness plugin did not claim the approval.'

        foreach ($client in $clients) {
            Act $profile @{ target = "$client/session-1"; text = $marker }
            Action-Receipt $profile $resident $client 'queued' | Out-Null
        }
        $receiptPath = Join-Path $profile 'adapters/delivery-receipts.json'
        $receipts = Json $receiptPath
        Require ($receipts.Count -eq 4) 'Expected exactly four persisted queue receipts.'
        $ids = @{}
        foreach ($client in $clients) {
            $delivery = @($receipts | Where-Object { $_.session.client -ceq $client })
            Require ($delivery.Count -eq 1 -and $delivery[0].session.id -ceq 'session-1') "Bad $client receipt."
            if ($client -ceq 'antigravity') {
                Require ($null -eq $delivery[0].externalId) 'The native Antigravity acknowledgement had no message ID.'
            } else {
                Require ($delivery[0].externalId) "Missing $client queue ID."
            }
            $ids[$client] = $delivery[0].id
        }
        $agWrites = Json "$ag/writes.json"
        Require ($agWrites.decisions.Count -eq 1 -and $agWrites.decisions[0].requestId -ceq 'request-1' -and
            $agWrites.decisions[0].trajectoryId -ceq 'trajectory-fixture' -and $agWrites.decisions[0].stepIndex -eq 3 -and
            $agWrites.decisions[0].cascadeId -ceq 'session-1' -and -not $agWrites.decisions[0].allow) 'Antigravity decision did not reach the original waiting step.'
        Require ($agWrites.deliveries.Count -eq 1 -and $agWrites.deliveries[0].cascadeId -ceq 'session-1') 'Antigravity delivery did not reach the original conversation.'
        $queue = Json "$codex/queue-receipt-$($ids.codex).json"
        Require ($queue.threadId -ceq 'session-1') 'Codex queued to a different thread.'
        $claudePending = "$claude/plugin/pending/session-1/$($ids.claude).json"
        $dshPending = "$dsh/prompts/$($ids.harness).json"
        Require ((Json $claudePending).text -ceq $marker -and (Json $dshPending).content[0].text -ceq $marker) 'Plugin queue did not contain the selected draft.'

        $agInput = @{ type = 'CORTEX_STEP_TYPE_USER_INPUT'; userInput = @{ items = @(@{ text = $marker }) } }
        Put "$ag/trajectory.json" @($agInput)
        Action-Receipt $profile $resident 'antigravity' 'received' | Out-Null
        Put "$ag/trajectory.json" @($agInput, @{ type = 'CORTEX_STEP_TYPE_PLANNER_RESPONSE' })
        Action-Receipt $profile $resident 'antigravity' 'started' | Out-Null

        Stop-Resident $profile $resident
        Remove-Item -LiteralPath (Join-Path $profile 'quit.request')
        foreach ($frame in Get-ChildItem -LiteralPath $profile -Filter '*.json' -File | Where-Object { $_.BaseName -match '^\d+$' }) {
            Remove-Item -LiteralPath $frame.FullName
        }
        $resident = Start-Fixture $profile 'restart' $exe @()
        Wait-View $profile $resident 'restarted strip' { param($v) $v.snapshot.surface -ceq 'strip' } | Out-Null
        Act $profile @{ open = $true }
        $restarted = Action-Receipt $profile $resident 'harness' 'queued'
        Require ($restarted.inboxView.deliveries.Count -eq 4 -and (Test-Path -LiteralPath $claudePending) -and
            (Test-Path -LiteralPath $dshPending)) 'Queued messages or receipts did not survive restart.'
        $claudeTimeline = @($restarted.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'claude/session-1' } | ForEach-Object { $_[1] })
        Require (($claudeTimeline -join ',') -ceq 'history-prompt-1,history-reply-1') 'Claude history was not rebuilt on restart.'
        Require ((Get-FileHash -LiteralPath $claudeHistory).Hash -ceq $claudeHistoryHash) 'Rooom modified the Claude source transcript.'
        $agTimeline = @($restarted.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'antigravity/session-1' } | ForEach-Object { $_[1] })
        Require (($agTimeline -join ',') -ceq 'step-1,step-3') 'Antigravity history was not rebuilt on restart.'
        Require ((Get-FileHash -LiteralPath $agHistory).Hash -ceq $agHistoryHash) 'Rooom modified the Antigravity source transcript.'
        $codexTimeline = @($restarted.inboxView.timeline | Where-Object { $_ -and $_[0] -ceq 'codex/session-1' } | ForEach-Object { $_[1] })
        Require (($codexTimeline -join ',') -ceq 'input:item-prompt,item-intro') 'Codex history was not rebuilt on restart.'
        Require ((Get-FileHash -LiteralPath $codexHistory).Hash -ceq $codexHistoryHash) 'Rooom modified the Codex source rollout.'
        Action-Receipt $profile $resident 'antigravity' 'started' | Out-Null
        # Hold a reader that denies delete sharing until a real replacement
        # failure is observed. Release it before the Host admits the prompt.
        $retriesBefore = @(Get-Content "$profile/harness.err.log" | Where-Object {
            $_ -match '"publication-retry"' -and $_ -match '"check-in.json"'
        }).Count
        $reader = [IO.File]::Open("$dsh/check-in.json", [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
        try {
            Wait-For 'Harness check-in replacement collision' {
                if ($dshHost.HasExited) { throw 'The Harness fixture Host exited during check-in replacement.' }
                @(Get-Content "$profile/harness.err.log" | Where-Object {
                    $_ -match '"publication-retry"' -and $_ -match '"check-in.json"'
                }).Count -gt $retriesBefore
            } $resident | Out-Null
        } finally {
            $reader.Dispose()
        }
        Marker "$claude/start-claim" $ids.claude
        Marker "$dsh/claim-prompts"
        Marker "$codex/delivery-events.jsonl" ('{"type":"queue_taken","thread_id":"session-1","queued_submission_id":"' + $queue.messageId + '","turn_id":"turn-send-1"}')
        foreach ($client in @('claude', 'codex', 'harness')) { Action-Receipt $profile $resident $client 'received' | Out-Null }
        Require ((Json "$claude/plugin-claim.json").count -eq 1 -and -not (Test-Path -LiteralPath $claudePending) -and
            -not (Test-Path -LiteralPath $dshPending)) 'A claimed prompt file survived or was claimed more than once.'
        Require ((Json "$dsh/controller-accepted.json").requestId -ceq $ids.harness -and
            (Json "$dsh/controller-accepted.json").count -eq 1) 'Harness recovery admitted a duplicate controller request.'
        Marker "$claude/start-turn" $ids.claude
        Marker "$dsh/start-turn" $ids.harness
        Marker "$codex/delivery-events.jsonl" ('{"type":"task_started","thread_id":"session-1","queued_submission_id":"' + $queue.messageId + '","turn_id":"turn-send-1"}')
        foreach ($client in @('claude', 'codex', 'harness')) { Action-Receipt $profile $resident $client 'started' | Out-Null }
        Remove-Item -LiteralPath "$dsh/claim-prompts"
        Marker "$dsh/stop-check-ins"
        Wait-For 'Harness check-ins stopping' { Test-Path -LiteralPath "$dsh/check-ins-stopped.json" } $resident | Out-Null
        $checkIn = Json "$dsh/check-in.json"; $checkIn.at = 1; Put "$dsh/check-in.json" $checkIn
        Put "$dsh/prompts/expired-fixture.json" @{
            requestId = 'expired-fixture'; sessionId = 'session-1'; mode = 'queue'
            content = @(@{ type = 'text'; text = $marker }); expiresAt = 1
        }
        Wait-View $profile $resident 'stale Harness check-in' {
            param($v) $v.inboxView.blocked -ccontains 'harness' -and $v.inboxView.sessions -cnotcontains 'harness/session-1'
        } | Out-Null
        Wait-For 'expired Harness prompt cleanup' { Test-Path -LiteralPath "$dsh/receipts/expired-fixture.json" } $resident | Out-Null
        Require (-not (Test-Path -LiteralPath "$dsh/prompts/expired-fixture.json") -and
            (Json "$dsh/receipts/expired-fixture.json").stage -ceq 'expired') 'An expired prompt was kept or delivered.'

        # Disable two channels and present unknown versions on the other two.
        $originals = @{}
        for ($i = 0; $i -lt $settings.Count; $i++) { $originals[$settings[$i]] = [IO.File]::ReadAllBytes($settings[$i]) }
        $record = Json "$claude/client.json"; $record.enabled = $false; Put "$claude/client.json" $record
        $record = Json "$codex/owner.json"; $record.desktopVersion = 'unknown'; Put "$codex/owner.json" $record
        $record = Json "$ag/hub.json"; $record.lsVersion = 'unknown'; Put "$ag/hub.json" $record
        $record = Json "$dsh/settings.json"; $record.enabled = $false; Put "$dsh/settings.json" $record
        Wait-View $profile $resident 'four disabled sources with no eligible sessions' {
            param($v) $v.inboxView.blocked.Count -eq 4 -and $v.inboxView.sessions.Count -eq 0 -and $v.inboxView.deliveries.Count -eq 4
        } | Out-Null
        Act $profile @{ probe = $true; targets = @($clients | ForEach-Object { @{client = $_; id = 'session-1'} }); text = $marker }
        Wait-For 'negative send probe' { Test-Path -LiteralPath "$profile/blocked-probe.json" } $resident | Out-Null
        $blocked = @(Json "$profile/blocked-probe.json")
        Require ($blocked.Count -eq 4 -and (@(Json $receiptPath)).Count -eq 4 -and
            (Json "$ag/writes.json").deliveries.Count -eq 1 -and
            @(Get-ChildItem -LiteralPath $codex -Filter 'queue-receipt-*.json').Count -eq 1 -and
            @(Get-ChildItem -LiteralPath "$dsh/prompts" -File).Count -eq 0 -and
            @(Get-ChildItem -LiteralPath "$claude/plugin/pending/session-1" -File).Count -eq 0
        ) 'Disabled or unknown sources issued a write.'
        foreach ($file in $settings) { [IO.File]::WriteAllBytes($file, $originals[$file]) }
        for ($i = 0; $i -lt $settings.Count; $i++) {
            Require ((Get-FileHash -LiteralPath $settings[$i]).Hash -ceq $hashes[$i]) 'A fixture client setting changed.'
        }
        Stop-Resident $profile $resident

        foreach ($name in @('missing', 'timeout')) {
            $check = Join-Path $root $name
            New-Profile $check @('claude')
            $runner = Start-Claude $check 1600
            $client = Join-Path $check 'adapters/claude'
            Wait-For "$name hook registry" { Test-Path -LiteralPath "$client/client.json" } | Out-Null
            $live = $null
            if ($name -ceq 'timeout') {
                $live = Start-Fixture $check 'resident' $exe @() 1600
                Wait-View $check $live 'timeout fixture resident' { param($v) $v.snapshot.surface -ceq 'strip' } | Out-Null
            }
            Marker "$client/start-hook"
            if ($live) {
                Wait-View $check $live 'unanswered hook card' { param($v) $v.inboxView.requests.Count -eq 1 } | Out-Null
            }
            Wait-For "$name hook return" { Test-Path -LiteralPath "$client/hook-exit.json" } | Out-Null
            Require ((Json "$client/hook-exit.json").exitCode -eq 0 -and
                (Get-Item -LiteralPath "$client/hook-stdout.json").Length -eq 0) "An absent or timed out hook produced a decision."
            if ($live) {
                Wait-View $check $live 'withdrawal of the timed out hook card' { param($v) $v.inboxView -and $v.inboxView.requests.Count -eq 0 } | Out-Null
                Stop-Resident $check $live
            }
            Marker "$client/stop-runner"
            Require ($runner.WaitForExit(5000) -and $runner.ExitCode -eq 0) 'Claude fixture runner did not stop.'
        }
        Marker "$claude/stop-runner"
        Marker "$dsh/stop-host"
        Require ($claudeHost.WaitForExit(5000) -and $claudeHost.ExitCode -eq 0 -and
            $dshHost.WaitForExit(5000) -and $dshHost.ExitCode -eq 0 -and -not $agHost.HasExited) 'A fixture client stopped unexpectedly.'
        # The server is blocked in AcceptTcpClient; stop this owned helper
        # before opening its redirected log files for the leakage check.
        $agHost.Kill()
        $agHost.WaitForExit()
        $diagnostics = @(Get-ChildItem -LiteralPath $root -Recurse -File | Where-Object {
            $_.Extension -ceq '.log' -or $_.Name -ceq 'delivery-receipts.json' -or $_.BaseName -match '^\d+$' -and $_.DirectoryName -notmatch '[\\/]sessions$'
        })
        Require ($diagnostics.Count -gt 0) 'Missing diagnostic artifacts.'
        foreach ($file in $diagnostics) {
            # Only diagnostic artifacts; web runtime caches and source files are not logs.
            if ($file.FullName -match '[\\/]webview[\\/]') { continue }
            $content = [IO.File]::ReadAllText($file.FullName)
            Require (-not ($content.Contains($marker) -or $content.Contains($secret) -or
                $content.Contains('ROOOM_CLAUDE_FIXTURE_TOOL') -or $content.Contains('Claude Workspace') -or
                $content.Contains('Codex Workspace') -or $content.Contains('Antigravity Workspace') -or
                $content.Contains('Harness Workspace'))) 'Fixture text, project names or credentials appeared in diagnostics.'
        }
        'Fixtures PASS: 4 sessions, 4 cards, 4 workspace labels, 4 routed sends, 4 queue receipts, 4 received and started; passive focus, timed-out/missing hook, restart, version gating and log checks.'
    } finally {
        foreach ($p in $processes) {
            if (-not $p.HasExited) { $p.Kill($true); $p.WaitForExit() }
            $p.Dispose()
        }
        $directory = Get-Item -LiteralPath $root
        if ($directory.FullName -cne $root -or $directory.Parent.FullName -cne $project -or
            ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            @(Get-ChildItem -LiteralPath $root -Recurse -Force -Attributes ReparsePoint).Count) {
            throw 'The fixture cleanup path is not the disposable directory.'
        }
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            try { Remove-Item -LiteralPath $root -Recurse -Force; break }
            catch [System.IO.IOException] { if ($attempt -eq 19) { throw }; Start-Sleep -Milliseconds 250 }
        }
        if (Test-Path -LiteralPath $root) { throw 'The disposable fixture directory was not removed.' }
    }
} finally {
    Pop-Location
}
