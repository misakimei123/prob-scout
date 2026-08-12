[CmdletBinding()]
param(
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$StartUtc = "2025-10-01 00:00:00",
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$EndUtc = "2025-11-15 00:00:00",
    [string]$EventPrefix = "2025 Season World Championship%",
    [ValidateRange(10, 50)]
    [int]$Limit = 10,
    [string]$OutputRoot = "",
    [switch]$Refresh
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($StartUtc -ge $EndUtc) {
    throw "StartUtc 必须早于 EndUtc。"
}
if ([string]::IsNullOrWhiteSpace($EventPrefix)) {
    throw "EventPrefix 不能为空。"
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/leaguepedia"
}

$sourceDirectory = Join-Path $OutputRoot "source"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $sourceDirectory, $manifestDirectory | Out-Null

$tables = "MatchSchedule=MS,TeamRedirects=TR1,Teams=T1,TournamentRosters=R1,TeamRedirects=TR2,Teams=T2,TournamentRosters=R2"
$fields = "MS.MatchId,MS.DateTime_UTC=MatchStartUtc,MS.Team1,TR1._pageName=Team1Page,T1.Short=Team1Short,MS.Team2,TR2._pageName=Team2Page,T2.Short=Team2Short,MS.Team1Score,MS.Team2Score,MS.Winner,MS.BestOf,MS.OverviewPage,MS._pageName=DataPage,R1.RosterLinks=Team1Roster,R2.RosterLinks=Team2Roster"
$where = 'MS.DateTime_UTC >= "{0}" AND MS.DateTime_UTC < "{1}" AND MS.OverviewPage LIKE "{2}"' -f $StartUtc, $EndUtc, $EventPrefix
$joinOn = "MS.Team1=TR1.AllName,TR1._pageName=T1._pageName,MS.PageAndTeam1=R1.PageAndTeam,MS.Team2=TR2.AllName,TR2._pageName=T2._pageName,MS.PageAndTeam2=R2.PageAndTeam"
$parameters = [ordered]@{
    "tables[0]" = $tables
    "fields[0]" = $fields
    "where[0]" = $where
    "join_on[0]" = $joinOn
    "group_by[0]" = "MS.MatchId"
    "order_by[0]" = "MS.DateTime_UTC DESC"
    "limit[0]" = [string]$Limit
    format = "json"
}

$queryPairs = foreach ($entry in $parameters.GetEnumerator()) {
    "{0}={1}" -f [uri]::EscapeDataString($entry.Key), [uri]::EscapeDataString([string]$entry.Value)
}
$requestUrl = "https://lol.fandom.com/wiki/Special:CargoExport?{0}" -f ($queryPairs -join "&")
$queryContract = $parameters | ConvertTo-Json -Compress
$queryHasher = [System.Security.Cryptography.SHA256]::Create()
try {
    $queryHashBytes = $queryHasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($queryContract))
}
finally {
    $queryHasher.Dispose()
}
$queryHash = ([System.BitConverter]::ToString($queryHashBytes) -replace '-', '').ToLowerInvariant()
$cachePath = Join-Path $manifestDirectory ("query-cache.{0}.json" -f $queryHash.Substring(0, 12))

$sourcePath = $null
$sourceHash = $null
$capturedAtUtc = $null
$responseStatusCode = $null
$responseContentType = $null
$sourceStatus = "downloaded"

# 默认复用同一查询合同下已通过 SHA-256 校验的响应，避免重复消耗公开 Cargo 服务。
if (-not $Refresh -and (Test-Path -LiteralPath $cachePath)) {
    $cache = Get-Content -Raw -LiteralPath $cachePath | ConvertFrom-Json
    if ($cache.query_sha256 -eq $queryHash -and (Test-Path -LiteralPath $cache.local_path)) {
        $cachedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cache.local_path).Hash.ToLowerInvariant()
        if ($cachedHash -eq $cache.response_sha256) {
            $sourcePath = [string]$cache.local_path
            $sourceHash = $cachedHash
            $capturedAtUtc = if ($cache.captured_at_utc -is [datetime]) {
                $cache.captured_at_utc.ToUniversalTime().ToString("o")
            }
            else {
                [string]$cache.captured_at_utc
            }
            $responseStatusCode = [int]$cache.http_status
            $responseContentType = [string]$cache.content_type
            $sourceStatus = "cached"
        }
    }
}

if ($null -eq $sourcePath) {
    $temporaryPath = Join-Path $sourceDirectory (".download-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        # 只发出一次官方 CargoExport 请求；不在脚本内并发、翻页或自动重试限流响应。
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $requestUrl `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (Leaguepedia Cargo sample)" } `
            -TimeoutSec 30 `
            -OutFile $temporaryPath `
            -PassThru

        $responseStatusCode = [int]$response.StatusCode
        $responseContentType = [string]$response.Headers["Content-Type"]
        if ($responseStatusCode -ne 200 -or $responseContentType -notmatch '^application/json') {
            throw "Leaguepedia CargoExport 未返回预期 JSON：HTTP $responseStatusCode，Content-Type=$responseContentType"
        }

        $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryPath).Hash.ToLowerInvariant()
        $sourcePath = Join-Path $sourceDirectory ("leaguepedia.{0}.json" -f $sourceHash.Substring(0, 12))
        $capturedAtUtc = (Get-Date).ToUniversalTime().ToString("o")

        if (Test-Path -LiteralPath $sourcePath) {
            Remove-Item -LiteralPath $temporaryPath
            $sourceStatus = "unchanged"
        }
        else {
            Move-Item -LiteralPath $temporaryPath -Destination $sourcePath
            $sourceStatus = "downloaded"
        }
    }
    catch {
        throw "Leaguepedia CargoExport 请求失败；不允许回退到 HTML scraper。原始错误：$($_.Exception.Message)"
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

try {
    $rows = @(Get-Content -Raw -LiteralPath $sourcePath | ConvertFrom-Json)
}
catch {
    throw "Leaguepedia 响应不是有效 JSON：$($_.Exception.Message)"
}

if ($rows.Count -lt 10) {
    throw "Leaguepedia 样本只有 $($rows.Count) 场，未达到至少 10 场的验收要求。"
}

$requiredFields = @(
    "MatchId",
    "MatchStartUtc",
    "Team1",
    "Team1Page",
    "Team2",
    "Team2Page",
    "OverviewPage",
    "Team1Roster",
    "Team2Roster"
)
$missingValues = [System.Collections.Generic.List[string]]::new()
$matchIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$teamPages = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)

for ($index = 0; $index -lt $rows.Count; $index++) {
    $row = $rows[$index]
    foreach ($field in $requiredFields) {
        if ($field -notin $row.PSObject.Properties.Name -or [string]::IsNullOrWhiteSpace([string]$row.$field)) {
            $missingValues.Add("row=$index field=$field")
        }
    }

    [void]$matchIds.Add([string]$row.MatchId)
    [void]$teamPages.Add([string]$row.Team1Page)
    [void]$teamPages.Add([string]$row.Team2Page)
}

if ($missingValues.Count -gt 0) {
    throw "Leaguepedia 样本缺少必需值：$($missingValues -join '; ')"
}
if ($matchIds.Count -ne $rows.Count) {
    throw "Leaguepedia 样本存在重复 MatchId：rows=$($rows.Count)，unique=$($matchIds.Count)"
}

$sourceFile = Get-Item -LiteralPath $sourcePath
$cacheRecord = [ordered]@{
    source_name = "Leaguepedia"
    source_page = "https://lol.fandom.com/wiki/Help:Leaguepedia_API"
    endpoint = "https://lol.fandom.com/wiki/Special:CargoExport"
    request_url = $requestUrl
    query_sha256 = $queryHash
    captured_at_utc = $capturedAtUtc
    http_status = $responseStatusCode
    content_type = $responseContentType
    local_path = $sourceFile.FullName
    response_sha256 = $sourceHash
    size_bytes = $sourceFile.Length
}
$cacheRecord | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath $cachePath

$manifest = [ordered]@{
    source = $cacheRecord
    source_status = $sourceStatus
    verified_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    query = [ordered]@{
        start_utc = $StartUtc
        end_utc_exclusive = $EndUtc
        event_prefix = $EventPrefix
        limit = $Limit
        order = "MatchStartUtc DESC"
    }
    sample = [ordered]@{
        rows = $rows.Count
        unique_matches = $matchIds.Count
        unique_team_pages = $teamPages.Count
        team_identifier_fields = @("Team1", "Team1Page", "Team1Short", "Team2", "Team2Page", "Team2Short")
        roster_separator = ";;"
        roster_complete_rows = @($rows | Where-Object {
                -not [string]::IsNullOrWhiteSpace([string]$_.Team1Roster) -and
                -not [string]::IsNullOrWhiteSpace([string]$_.Team2Roster)
            }).Count
        first_match_start_utc = ($rows | Sort-Object MatchStartUtc | Select-Object -First 1).MatchStartUtc
        last_match_start_utc = ($rows | Sort-Object MatchStartUtc | Select-Object -Last 1).MatchStartUtc
        fields = @($rows[0].PSObject.Properties.Name)
    }
}
$manifestPath = Join-Path $manifestDirectory ("leaguepedia.{0}.manifest.json" -f $queryHash.Substring(0, 12))
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    SourceStatus = $sourceStatus
    CapturedAtUtc = $capturedAtUtc
    QuerySha256 = $queryHash
    ResponseSha256 = $sourceHash
    Rows = $rows.Count
    UniqueMatches = $matchIds.Count
    UniqueTeamPages = $teamPages.Count
    RosterCompleteRows = $manifest.sample.roster_complete_rows
    Manifest = $manifestPath
}
