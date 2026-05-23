[CmdletBinding()]
param(
    [string]$QueryHost = "127.0.0.1",
    [int]$QueryPort = 10101,
    [string]$Login = "serveradmin",
    [string]$Password = "serveradmin",
    [int]$VirtualServerId = 1,
    [int]$ServerAdminGroupId = 0,
    [string]$Description = "Server Admin Grant",
    [int]$MaxUses = 1,
    [switch]$PassThruToken
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Encode-QueryValue {
    param([Parameter(Mandatory = $true)][string]$Value)

    $builder = New-Object System.Text.StringBuilder
    foreach ($character in $Value.ToCharArray()) {
        switch ($character) {
            '\\' { [void]$builder.Append('\\\\') }
            ' ' { [void]$builder.Append('\\s') }
            '|' { [void]$builder.Append('\\p') }
            '/' { [void]$builder.Append('\\/') }
            "`n" { [void]$builder.Append('\\n') }
            "`t" { [void]$builder.Append('\\t') }
            default { [void]$builder.Append($character) }
        }
    }

    $builder.ToString()
}

function Read-NonEmptyLine {
    param([Parameter(Mandatory = $true)][System.IO.StreamReader]$Reader)

    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) {
            throw "Query connection closed unexpectedly."
        }

        if ($line.Length -gt 0) {
            return $line
        }
    }
}

function Read-Banner {
    param([Parameter(Mandatory = $true)][System.IO.StreamReader]$Reader)

    while ($true) {
        $line = $Reader.ReadLine()
        if ($null -eq $line) {
            throw "Query connection closed before banner completed."
        }

        if ($line.Length -eq 0) {
            return
        }
    }
}

function Read-QueryResponse {
    param([Parameter(Mandatory = $true)][System.IO.StreamReader]$Reader)

    $lines = New-Object System.Collections.Generic.List[string]

    while ($true) {
        $line = Read-NonEmptyLine -Reader $Reader
        [void]$lines.Add($line)
        if ($line -like 'error id=*') {
            return $lines.ToArray()
        }
    }
}

function Send-QueryCommand {
    param(
        [Parameter(Mandatory = $true)][System.IO.StreamWriter]$Writer,
        [Parameter(Mandatory = $true)][System.IO.StreamReader]$Reader,
        [Parameter(Mandatory = $true)][string]$Command
    )

    $Writer.Write($Command + "`r`n")
    $Writer.Flush()
    Read-QueryResponse -Reader $Reader
}

function Assert-QuerySuccess {
    param(
        [Parameter(Mandatory = $true)][string[]]$ResponseLines,
        [Parameter(Mandatory = $true)][string]$CommandName
    )

    $errorLine = $ResponseLines[-1]
    if ($errorLine -notmatch '^error id=(\d+) msg=') {
        throw "Unexpected response format for ${CommandName}: $($ResponseLines -join "`n")"
    }

    $errorId = [int]$Matches[1]
    if ($errorId -ne 0) {
        throw "${CommandName} failed: $($ResponseLines -join "`n")"
    }
}

function Get-QueryFieldValue {
    param(
        [Parameter(Mandatory = $true)][string]$Line,
        [Parameter(Mandatory = $true)][string]$FieldName
    )

    $match = [regex]::Match($Line, '(?:^|[\s|])' + [regex]::Escape($FieldName) + '=([^\s|]+)')
    if ($match.Success) {
        return $match.Groups[1].Value
    }

    return $null
}

function Resolve-ServerAdminGroupId {
    param(
        [Parameter(Mandatory = $true)][System.IO.StreamWriter]$Writer,
        [Parameter(Mandatory = $true)][System.IO.StreamReader]$Reader
    )

    $instanceInfo = Send-QueryCommand -Writer $Writer -Reader $Reader -Command 'instanceinfo'
    Assert-QuerySuccess -ResponseLines $instanceInfo -CommandName 'instanceinfo'

    $groupId = Get-QueryFieldValue -Line $instanceInfo[0] -FieldName 'serverinstance_template_serveradmin_group'
    if ($groupId) {
        return [int]$groupId
    }

    $groupList = Send-QueryCommand -Writer $Writer -Reader $Reader -Command 'servergrouplist'
    Assert-QuerySuccess -ResponseLines $groupList -CommandName 'servergrouplist'

    foreach ($groupRow in $groupList[0] -split '\|') {
        $groupName = Get-QueryFieldValue -Line $groupRow -FieldName 'name'
        if ($groupName -eq 'Server\sAdmin') {
            $resolvedId = Get-QueryFieldValue -Line $groupRow -FieldName 'sgid'
            if (-not $resolvedId) {
                break
            }

            return [int]$resolvedId
        }
    }

    throw 'Server Admin group id could not be resolved via instanceinfo or servergrouplist.'
}

$client = [System.Net.Sockets.TcpClient]::new()

try {
    $client.Connect($QueryHost, $QueryPort)
    $stream = $client.GetStream()
    $stream.ReadTimeout = 30000
    $stream.WriteTimeout = 30000

    $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::UTF8)
    $writer = [System.IO.StreamWriter]::new($stream, [System.Text.Encoding]::UTF8)
    $writer.NewLine = "`r`n"
    $writer.AutoFlush = $true

    Read-Banner -Reader $reader

    $loginResponse = Send-QueryCommand -Writer $writer -Reader $reader -Command (
        'login ' + (Encode-QueryValue -Value $Login) + ' ' + (Encode-QueryValue -Value $Password)
    )
    Assert-QuerySuccess -ResponseLines $loginResponse -CommandName 'login'

    $useResponse = Send-QueryCommand -Writer $writer -Reader $reader -Command "use sid=$VirtualServerId"
    Assert-QuerySuccess -ResponseLines $useResponse -CommandName 'use'

    if ($ServerAdminGroupId -le 0) {
        $ServerAdminGroupId = Resolve-ServerAdminGroupId -Writer $writer -Reader $reader
    }

    $tokenCommand = @(
        'privilegekeyadd',
        ('token_description=' + (Encode-QueryValue -Value $Description)),
        "token_max_uses=$MaxUses",
        'action_type=2',
        "action_id1=$ServerAdminGroupId"
    ) -join ' '

    $tokenResponse = Send-QueryCommand -Writer $writer -Reader $reader -Command $tokenCommand
    Assert-QuerySuccess -ResponseLines $tokenResponse -CommandName 'privilegekeyadd'

    $rowLine = $tokenResponse[0]
    $token = Get-QueryFieldValue -Line $rowLine -FieldName 'token'
    $tokenId = Get-QueryFieldValue -Line $rowLine -FieldName 'token_id'
    $actionId = Get-QueryFieldValue -Line $rowLine -FieldName 'action_id'

    if (-not $token) {
        throw "privilegekeyadd succeeded but no token field was returned: $rowLine"
    }

    if ($PassThruToken) {
        $token
        return
    }

    [pscustomobject]@{
        QueryHost = $QueryHost
        QueryPort = $QueryPort
        VirtualServerId = $VirtualServerId
        ServerAdminGroupId = $ServerAdminGroupId
        Token = $token
        TokenId = $tokenId
        ActionId = $actionId
        RawResponse = $rowLine
    }
}
finally {
    if ($client.Connected) {
        $client.Close()
    }
}