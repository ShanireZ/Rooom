param(
    [Parameter(Mandatory)][string]$Root,
    [ValidateSet('normal', 'foreign', 'lostSend', 'native', 'native2322', 'nativeLost')][string]$Mode = 'normal'
)

$ErrorActionPreference = 'Stop'
if (-not [IO.Path]::IsPathFullyQualified($Root) -or -not [IO.Directory]::Exists($Root)) {
    throw 'The Antigravity fixture requires an existing disposable directory.'
}

function Write-Receipt([string]$Name, $Value) {
    $path = Join-Path $Root $Name
    [IO.File]::WriteAllText("$path.tmp", (ConvertTo-Json -InputObject $Value -Depth 12 -Compress))
    [IO.File]::Move("$path.tmp", $path, $true)
}

function Read-Request($Stream) {
    $header = [Collections.Generic.List[byte]]::new()
    $tail = 0
    while ($tail -ne 0x0d0a0d0a) {
        $byte = $Stream.ReadByte()
        if ($byte -lt 0 -or $header.Count -ge 4096) { return }
        $header.Add([byte]$byte)
        $tail = ($tail -shl 8) -bor $byte
    }
    $lines = [Text.Encoding]::ASCII.GetString($header.ToArray()) -split '\r\n'
    if ($lines[0] -notmatch '^POST /exa\.language_server_pb\.LanguageServerService/([A-Za-z]+) HTTP/1\.1$') {
        return
    }
    $method = $Matches[1]
    $headers = @{}
    foreach ($line in $lines | Select-Object -Skip 1) {
        if ($line -match '^([^:]+):\s*(.*)$') {
            if ($headers.ContainsKey($Matches[1])) { return }
            $headers[$Matches[1]] = $Matches[2]
        }
    }
    $length = 0
    if ($headers['X-Codeium-Csrf-Token'] -cne $token -or
        $headers['Connect-Protocol-Version'] -cne '1' -or
        -not [int]::TryParse($headers['Content-Length'], [ref]$length) -or
        $length -lt 2 -or $length -gt 131072) {
        return
    }
    $body = [byte[]]::new($length)
    $Stream.ReadExactly($body, 0, $body.Length)
    $parameters = [Text.Encoding]::UTF8.GetString($body) | ConvertFrom-Json -AsHashtable
    @{ Method = $method; Parameters = $parameters }
}

function Write-Response($Stream, $Value, [int]$ChunkSize = 37) {
    $bytes = [Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $Value -Depth 12 -Compress))
    if ($native) {
        $encoding = [Text.Encoding]::ASCII
        $Stream.Write($encoding.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nTransfer-Encoding: chunked`r`nConnection: close`r`n`r`n"))
        for ($offset = 0; $offset -lt $bytes.Length; $offset += $size) {
            $size = [Math]::Min($ChunkSize, $bytes.Length - $offset)
            $Stream.Write($encoding.GetBytes($size.ToString('x') + "`r`n"))
            $Stream.Write($bytes, $offset, $size)
            $Stream.Write($encoding.GetBytes("`r`n"))
        }
        $Stream.Write($encoding.GetBytes("0`r`n`r`n"))
        return
    }
    $header = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bytes.Length)`r`nConnection: close`r`n`r`n")
    $Stream.Write($header, 0, $header.Length)
    $Stream.Write($bytes, 0, $bytes.Length)
}

$instance = [Guid]::NewGuid().ToString('N')
$token = 'fixture-csrf-' + [Guid]::NewGuid().ToString('N')
$identity = @{
    pid = $PID
    desktopVersion = '2.29.0'
    lsVersion = '2.31.0'
    instanceId = $instance
}
$status = $identity.Clone()
$status.userStatus = @{ signedIn = $true }
if ($Mode -eq 'foreign') { $status.instanceId = 'wrong-fixture-service' }
$summaries = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'summaries.json') -Raw | ConvertFrom-Json -AsHashtable
$summary = $summaries.trajectorySummaries['session-1']
$writes = @{
    decisions = [Collections.Generic.List[object]]::new()
    deliveries = [Collections.Generic.List[object]]::new()
}
$model = 'fixture-model'
$native = $Mode -in @('native', 'native2322', 'nativeLost')
if ($native) {
    $identity.desktopVersion = if ($Mode -eq 'native2322') { '2.32.2' } else { '2.32.1' }
    $identity.lsVersion = $identity.desktopVersion
    $status = @{ userStatus = @{ cascadeModelConfigData = @{
        defaultOverrideModelConfig = @{ modelOrAlias = @{ model = 'MODEL_FIXTURE' } }
    } } }
}
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$listener.Server.ExclusiveAddressUse = $true
$listener.Start()
try {
    Write-Receipt 'writes.json' $writes
    Write-Receipt 'hub.json' ($identity + @{
        httpPort = $listener.LocalEndpoint.Port
        csrfToken = $token
        issuedAtMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        enabled = $true
    })
    while ($true) {
        $client = $listener.AcceptTcpClient()
        try {
            $client.ReceiveTimeout = 750
            $client.SendTimeout = 750
            $stream = $client.GetStream()
            $request = Read-Request $stream
            if ($null -eq $request) { continue }
            $parameters = $request.Parameters
            switch -CaseSensitive ($request.Method) {
                'GetUserStatus' {
                    if ($native -and $parameters.metadata.ideVersion -cne $identity.desktopVersion) { continue }
                    Write-Response $stream $status
                }
                'GetAllCascadeTrajectories' {
                    if ($summary.waitingSteps.Count -eq 1 -and
                        [IO.File]::Exists((Join-Path $Root 'change.request'))) {
                        $summary.waitingSteps[0].step.requestedInteraction.permission.resource.target = 'fixture-antigravity-changed-command'
                    }
                    Write-Response $stream $summaries
                }
                'GetCascadeTrajectory' {
                    if ($parameters.cascadeId -ceq 'session-1') {
                        if ($native) {
                            $steps = Get-Content -LiteralPath (Join-Path $Root 'trajectory.json') -Raw | ConvertFrom-Json -AsHashtable -NoEnumerate
                            Write-Response $stream @{
                                trajectory = @{ trajectoryId = 'trajectory-fixture'; steps = $steps }
                                metadata = @{ syntheticPadding = 'x' * (300 * 1024) }
                            } 4096
                            continue
                        }
                        Write-Response $stream @{
                            cascadeId = 'session-1'
                            cascadeConfig = @{ plannerConfig = @{ model = $model } }
                        }
                    }
                }
                'HandleCascadeUserInteraction' {
                    $interaction = $parameters.interaction
                    if ($summary.waitingSteps.Count -ne 1 -or $parameters.cascadeId -cne 'session-1' -or
                        $interaction.trajectoryId -cne 'trajectory-fixture' -or $interaction.stepIndex -ne 3 -or
                        $interaction.permission.scope -cne 'PERMISSION_SCOPE_ONCE' -or
                        $interaction.permission.allow -isnot [bool]) { continue }
                    $decision = @{
                        cascadeId = 'session-1'
                        requestId = 'request-1'
                        trajectoryId = 'trajectory-fixture'
                        stepIndex = 3
                        allow = $interaction.permission.allow
                    }
                    $writes.decisions.Add($decision)
                    $summary.waitingSteps = @()
                    Write-Receipt 'writes.json' $writes
                    Write-Response $stream $decision
                }
                'SendUserCascadeMessage' {
                    if ($parameters.cascadeId -cne 'session-1' -or
                        $parameters.deliveryStrategy -cne 'MESSAGE_DELIVERY_STRATEGY_WHEN_IDLE' -or
                        $parameters.items.Count -ne 1 -or [string]::IsNullOrWhiteSpace($parameters.items[0].text)) { continue }
                    if ($native) {
                        if ($parameters.cascadeConfig.plannerConfig.requestedModel.model -cne 'MODEL_FIXTURE' -or
                            $parameters.metadata.ideVersion -cne $identity.desktopVersion -or
                            $parameters.Contains('rooomDeliveryId')) { continue }
                        $writes.deliveries.Add(@{ cascadeId = 'session-1' })
                        Write-Receipt 'writes.json' $writes
                        if ($Mode -ne 'nativeLost') { Write-Response $stream @{} }
                        continue
                    }
                    if ($parameters.cascadeConfig.plannerConfig.model -cne $model -or
                        $parameters.rooomDeliveryId -cnotmatch '^[a-zA-Z0-9_-]{1,128}$') { continue }
                    $delivery = @{
                        cascadeId = 'session-1'
                        deliveryId = $parameters.rooomDeliveryId
                        messageId = "antigravity-message-$($writes.deliveries.Count + 1)"
                    }
                    $writes.deliveries.Add($delivery)
                    Write-Receipt 'writes.json' $writes
                    if ($Mode -ne 'lostSend') {
                        Write-Response $stream ($delivery + @{ state = 'queued' })
                    }
                }
            }
        } catch [IO.IOException] {
            # A disconnected fixture peer does not stop the service.
        } catch [System.ArgumentException] {
            # A malformed fixture call receives no reply.
        } finally {
            $client.Dispose()
        }
    }
} finally {
    $listener.Stop()
}
