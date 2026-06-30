param(
    [string]$Configuration = "debug"
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), 0)
    $listener.Start()
    try {
        return $listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Get-TextWithRetry {
    param(
        [string]$Uri,
        [int]$Attempts = 30
    )

    $lastError = $null
    for ($i = 0; $i -lt $Attempts; $i++) {
        try {
            $client = [System.Net.WebClient]::new()
            try {
                return $client.DownloadString($Uri)
            }
            finally {
                $client.Dispose()
            }
        }
        catch {
            $lastError = $_
            Start-Sleep -Milliseconds 100
        }
    }

    throw $lastError
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$exePath = Join-Path $repoRoot "target\$Configuration\yiz-tunnel.exe"
if (-not (Test-Path $exePath)) {
    Push-Location $repoRoot
    try {
        cargo build
    }
    finally {
        Pop-Location
    }
}

Assert-True (Test-Path $exePath) "missing executable: $exePath"

$runId = [Guid]::NewGuid().ToString("N")
$runDir = Join-Path $repoRoot "target\smoke-api-$runId"
$dataDir = Join-Path $runDir "data"
$logDir = Join-Path $runDir "logs"
$publicDir = Join-Path $runDir "public"
$configPath = Join-Path $runDir "yiz-tunnel.json"
$stdoutPath = Join-Path $runDir "stdout.log"
$stderrPath = Join-Path $runDir "stderr.log"

New-Item -ItemType Directory -Force -Path $dataDir, $logDir, $publicDir | Out-Null
Set-Content -Path (Join-Path $publicDir "hello.txt") -Value "hello smoke" -NoNewline

$adminPort = Get-FreeTcpPort
$runtimePort = Get-FreeTcpPort
$baseUrl = "http://127.0.0.1:$adminPort/api/v1"

$systemConfig = [ordered]@{
    version = 1
    "data-dir" = "./data"
    "log-dir" = "./logs"
    admin = [ordered]@{
        host = "127.0.0.1"
        port = $adminPort
    }
    runtime = @{}
}
$systemConfig | ConvertTo-Json -Depth 10 | Set-Content -Path $configPath

$process = $null
try {
    $process = Start-Process -FilePath $exePath `
        -ArgumentList "-c", $configPath `
        -WorkingDirectory $runDir `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath `
        -WindowStyle Hidden `
        -PassThru

    $ready = $false
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        try {
            $status = Invoke-RestMethod -Uri "$baseUrl/system/status" -TimeoutSec 2
            if ($status.code -eq 0) {
                $ready = $true
                break
            }
        }
        catch {
        }
    }
    Assert-True $ready "management API did not become ready"

    function Invoke-Api {
        param(
            [string]$Method,
            [string]$Path,
            [object]$Body = $null
        )

        $parameters = @{
            Method = $Method
            Uri = "$baseUrl$Path"
            TimeoutSec = 5
        }
        if ($null -ne $Body) {
            $parameters.ContentType = "application/json"
            $parameters.Body = ($Body | ConvertTo-Json -Depth 20)
        }

        $response = Invoke-RestMethod @parameters
        Assert-True ($response.code -eq 0) "API failed: $Method $Path => $($response.message)"
        return $response.data
    }

    $server = Invoke-Api -Method "POST" -Path "/http-servers" -Body ([ordered]@{
        alias = "smoke"
        listen = [ordered]@{
            host = "127.0.0.1"
            port = $runtimePort
            serverName = @("localhost")
        }
        conf = @{}
        graceful = [ordered]@{
            enabled = $true
            type = 0
        }
    })
    Assert-True ($server.id -like "hs_*") "http-server id was not generated"

    $route = Invoke-Api -Method "POST" -Path "/http-server/$($server.id)/routes" -Body ([ordered]@{
        match = [ordered]@{
            type = 1
            path = "/"
        }
        action = [ordered]@{
            type = "file"
            file = [ordered]@{
                dir = $publicDir
                alias = 0
            }
        }
        conf = @{}
    })
    Assert-True ($route.id -like "rt_*") "route id was not generated"

    $fileBody = Get-TextWithRetry -Uri "http://127.0.0.1:$runtimePort/hello.txt"
    Assert-True ($fileBody -eq "hello smoke") "runtime file response body mismatch"

    $firstUpstream = Invoke-Api -Method "POST" -Path "/http-server/$($server.id)/upstreams" -Body ([ordered]@{
        group = "api"
        name = "v1"
        host = "http://127.0.0.1:3000"
        priority = 0
        conf = @{}
    })
    $secondUpstream = Invoke-Api -Method "POST" -Path "/http-server/$($server.id)/upstreams" -Body ([ordered]@{
        group = "api"
        name = "v1"
        host = "http://127.0.0.1:3001"
        priority = 0
        conf = @{}
    })
    Assert-True ($firstUpstream.id -ne $secondUpstream.id) "replacement upstream id did not change"

    $upstreams = Invoke-Api -Method "GET" -Path "/http-server/$($server.id)/upstreams"
    $matching = @($upstreams | Where-Object { $_.group -eq "api" -and $_.name -eq "v1" -and $_.status -eq "running" })
    Assert-True ($matching.Count -eq 1) "expected one running upstream after replacement"
    Assert-True ($matching[0].host -eq "http://127.0.0.1:3001") "replacement upstream host mismatch"

    $invalidBody = [ordered]@{
        alias = "bad-conf"
        listen = [ordered]@{
            host = "127.0.0.1"
            port = (Get-FreeTcpPort)
            serverName = @("localhost")
        }
        conf = [ordered]@{
            unknown = 1
        }
        graceful = [ordered]@{
            enabled = $true
            type = 0
        }
    } | ConvertTo-Json -Depth 20

    $invalidRejected = $false
    try {
        Invoke-RestMethod -Method "POST" -Uri "$baseUrl/http-servers" -ContentType "application/json" -Body $invalidBody -TimeoutSec 5 | Out-Null
    }
    catch {
        $statusCode = $_.Exception.Response.StatusCode.value__
        $invalidRejected = ($statusCode -eq 400)
    }
    Assert-True $invalidRejected "invalid conf was not rejected with 400"

    Write-Output "smoke-management-api passed"
    Write-Output "runDir=$runDir"
    Write-Output "adminPort=$adminPort runtimePort=$runtimePort"
}
finally {
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
    }
}
