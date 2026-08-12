[CmdletBinding()]
param(
    [string]$AsOfUtc = "",
    [ValidateRange(10, 50)]
    [int]$LimitPerScope = 20,
    [string]$OutputRoot = "",
    [switch]$Refresh,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Refresh -and $Offline) {
    throw "Refresh 与 Offline 不能同时使用。"
}

if ([string]::IsNullOrWhiteSpace($AsOfUtc)) {
    # 默认按当前 UTC 整点建立快照，使同一小时内重复运行可以命中同一份 cache。
    $AsOfUtc = "{0:yyyy-MM-ddTHH}:00:00Z" -f (Get-Date).ToUniversalTime()
}

$parsedAsOf = [datetimeoffset]::MinValue
$validAsOf = [datetimeoffset]::TryParseExact(
    $AsOfUtc,
    "yyyy-MM-ddTHH:mm:ss'Z'",
    [System.Globalization.CultureInfo]::InvariantCulture,
    [System.Globalization.DateTimeStyles]::AssumeUniversal,
    [ref]$parsedAsOf
)
if (-not $validAsOf) {
    throw "AsOfUtc 必须使用 yyyy-MM-ddTHH:mm:ssZ 格式。"
}
$AsOfUtc = $parsedAsOf.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/polymarket_gamma"
}

$sourceDirectory = Join-Path $OutputRoot "source"
$fixtureDirectory = Join-Path $OutputRoot "fixture"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $sourceDirectory, $fixtureDirectory, $manifestDirectory | Out-Null

$endpoint = "https://gamma-api.polymarket.com/events/keyset"
$lolTagId = 65
$lolSeriesId = 10311
$scopes = @(
    [pscustomobject][ordered]@{
        Name = "future"
        Closed = "false"
        DateParameter = "end_date_min"
        Ascending = "true"
    },
    [pscustomobject][ordered]@{
        Name = "historical"
        Closed = "true"
        DateParameter = "end_date_max"
        Ascending = "false"
    }
)

$sourceRecords = [System.Collections.Generic.List[object]]::new()
$allCandidates = [System.Collections.Generic.List[object]]::new()

foreach ($catalogScope in $scopes) {
    $parameters = [ordered]@{
        limit = [string]$LimitPerScope
        tag_id = [string]$lolTagId
        closed = $catalogScope.Closed
        $catalogScope.DateParameter = $AsOfUtc
        order = "endDate"
        ascending = $catalogScope.Ascending
    }
    $queryPairs = foreach ($entry in $parameters.GetEnumerator()) {
        "{0}={1}" -f [uri]::EscapeDataString($entry.Key), [uri]::EscapeDataString([string]$entry.Value)
    }
    $requestUrl = "{0}?{1}" -f $endpoint, ($queryPairs -join "&")
    $queryContract = $parameters | ConvertTo-Json -Compress
    $queryHasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $queryHashBytes = $queryHasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($queryContract))
    }
    finally {
        $queryHasher.Dispose()
    }
    $queryHash = ([System.BitConverter]::ToString($queryHashBytes) -replace '-', '').ToLowerInvariant()
    $cachePath = Join-Path $manifestDirectory ("query-cache.{0}.{1}.json" -f $catalogScope.Name, $queryHash.Substring(0, 12))

    $sourcePath = $null
    $sourceHash = $null
    $capturedAtUtc = $null
    $responseStatusCode = $null
    $responseContentType = $null
    $sourceStatus = "downloaded"

    # 每个 scope 默认复用经过 SHA-256 校验的完整响应，避免重复调用公开目录 API。
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

    if ($null -eq $sourcePath -and $Offline) {
        throw "Offline 模式缺少 $($catalogScope.Name) scope 的有效 cache：$cachePath"
    }

    if ($null -eq $sourcePath) {
        $temporaryPath = Join-Path $sourceDirectory (".download-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
        try {
            # future / historical 各发送一次只读请求；不自动翻页，也不实现隐藏 retry。
            $response = Invoke-WebRequest `
                -UseBasicParsing `
                -Uri $requestUrl `
                -Headers @{ "User-Agent" = "prob-scout-research/0.1 (Polymarket Gamma catalog sample)" } `
                -TimeoutSec 45 `
                -OutFile $temporaryPath `
                -PassThru

            $responseStatusCode = [int]$response.StatusCode
            $responseContentType = [string]$response.Headers["Content-Type"]
            if ($responseStatusCode -ne 200 -or $responseContentType -notmatch '^application/json') {
                throw "Gamma API 未返回预期 JSON：HTTP $responseStatusCode，Content-Type=$responseContentType"
            }

            $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryPath).Hash.ToLowerInvariant()
            $sourcePath = Join-Path $sourceDirectory ("events-{0}.{1}.json" -f $catalogScope.Name, $sourceHash.Substring(0, 12))
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
            throw "Polymarket Gamma $($catalogScope.Name) 目录请求失败：$($_.Exception.Message)"
        }
        finally {
            if (Test-Path -LiteralPath $temporaryPath) {
                Remove-Item -LiteralPath $temporaryPath
            }
        }
    }

    try {
        $payload = Get-Content -Raw -LiteralPath $sourcePath | ConvertFrom-Json
    }
    catch {
        throw "Gamma $($catalogScope.Name) 响应不是有效 JSON：$($_.Exception.Message)"
    }
    if ("events" -notin $payload.PSObject.Properties.Name) {
        throw "Gamma $($catalogScope.Name) 响应缺少 events 字段。"
    }

    $events = @($payload.events)
    if ($events.Count -eq 0) {
        throw "Gamma $($catalogScope.Name) 响应没有 LOL events。"
    }

    $scopeCandidates = [System.Collections.Generic.List[object]]::new()
    foreach ($event in $events) {
        if ([string]$event.title -notmatch '^LoL: .+ vs .+ \(BO[135]\) - ') {
            continue
        }

        foreach ($market in @($event.markets)) {
            # sportsMarketType=moneyline 是系列赛胜者；child_moneyline、totals 和 handicap 必须排除。
            if ([string]$market.sportsMarketType -ne "moneyline") {
                continue
            }

            try {
                $outcomes = @([string]$market.outcomes | ConvertFrom-Json)
                $tokenIds = @([string]$market.clobTokenIds | ConvertFrom-Json)
            }
            catch {
                throw "Market $($market.id) 的 outcomes 或 clobTokenIds 不是有效 JSON array。"
            }
            if ($outcomes.Count -ne 2 -or $tokenIds.Count -ne 2) {
                throw "Market $($market.id) 必须恰好包含两个 outcomes 和两个 CLOB token IDs。"
            }
            if (
                [string]::IsNullOrWhiteSpace([string]$event.id) -or
                [string]::IsNullOrWhiteSpace([string]$market.id) -or
                [string]::IsNullOrWhiteSpace([string]$market.conditionId) -or
                [string]::IsNullOrWhiteSpace([string]$outcomes[0]) -or
                [string]::IsNullOrWhiteSpace([string]$outcomes[1]) -or
                [string]::IsNullOrWhiteSpace([string]$tokenIds[0]) -or
                [string]::IsNullOrWhiteSpace([string]$tokenIds[1])
            ) {
                throw "Gamma Match Winner 候选缺少 event、market、condition 或 token ID。"
            }

            $eventEndDateUtc = if ($event.endDate -is [datetime]) {
                $event.endDate.ToUniversalTime().ToString("o")
            }
            else {
                [string]$event.endDate
            }

            $candidate = [pscustomobject][ordered]@{
                scope = $catalogScope.Name
                event_id = [string]$event.id
                event_slug = [string]$event.slug
                event_title = [string]$event.title
                event_end_date_utc = $eventEndDateUtc
                event_closed = [bool]$event.closed
                market_id = [string]$market.id
                market_slug = [string]$market.slug
                market_question = [string]$market.question
                sports_market_type = [string]$market.sportsMarketType
                condition_id = [string]$market.conditionId
                outcomes = $outcomes
                clob_token_ids = $tokenIds
                active = [bool]$market.active
                closed = [bool]$market.closed
                accepting_orders = [bool]$market.acceptingOrders
                enable_order_book = [bool]$market.enableOrderBook
            }
            $scopeCandidates.Add($candidate)
            $allCandidates.Add($candidate)
        }
    }

    if ($scopeCandidates.Count -eq 0) {
        throw "Gamma $($catalogScope.Name) 响应中没有 sportsMarketType=moneyline 的 LOL BO1/BO3/BO5 候选。"
    }
    $duplicateMarketIds = @($scopeCandidates | Group-Object market_id | Where-Object { $_.Count -gt 1 })
    if ($duplicateMarketIds.Count -gt 0) {
        throw "Gamma $($catalogScope.Name) 候选存在重复 market ID：$($duplicateMarketIds.Name -join ', ')"
    }
    if ($catalogScope.Name -eq "future") {
        $openCandidates = @($scopeCandidates | Where-Object { -not $_.closed -and $_.accepting_orders })
        if ($openCandidates.Count -eq 0) {
            throw "Gamma future 响应没有仍在接受订单的 Match Winner 候选。"
        }
    }

    $sourceFile = Get-Item -LiteralPath $sourcePath
    $cacheRecord = [ordered]@{
        scope = $catalogScope.Name
        endpoint = $endpoint
        request_url = $requestUrl
        query_sha256 = $queryHash
        captured_at_utc = $capturedAtUtc
        http_status = $responseStatusCode
        content_type = $responseContentType
        local_path = $sourceFile.FullName
        response_sha256 = $sourceHash
        size_bytes = $sourceFile.Length
        source_status = $sourceStatus
        raw_events = $events.Count
        match_winner_candidates = $scopeCandidates.Count
        next_cursor_present = -not [string]::IsNullOrWhiteSpace([string]$payload.next_cursor)
    }
    $cacheRecord | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 -LiteralPath $cachePath
    $sourceRecords.Add([pscustomobject]$cacheRecord)
}

$futureCandidates = @($allCandidates | Where-Object { $_.scope -eq "future" })
$historicalCandidates = @($allCandidates | Where-Object { $_.scope -eq "historical" })
$fixture = [ordered]@{
    source = "Polymarket Gamma API"
    endpoint = $endpoint
    as_of_utc = $AsOfUtc
    lol_tag_id = $lolTagId
    lol_series_id = $lolSeriesId
    match_winner_rule = [ordered]@{
        event_title_regex = '^LoL: .+ vs .+ \(BO[135]\) - '
        sports_market_type = "moneyline"
        excluded_market_types = @("child_moneyline", "totals", "map_handicap")
    }
    source_responses = @($sourceRecords | ForEach-Object {
            [ordered]@{
                scope = $_.scope
                captured_at_utc = $_.captured_at_utc
                query_sha256 = $_.query_sha256
                response_sha256 = $_.response_sha256
            }
        })
    candidates = @($allCandidates)
}

$temporaryFixturePath = Join-Path $fixtureDirectory (".catalog-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
try {
    $fixture | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $temporaryFixturePath
    $fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryFixturePath).Hash.ToLowerInvariant()
    $fixturePath = Join-Path $fixtureDirectory ("lol-match-winner.{0}.json" -f $fixtureHash.Substring(0, 12))
    $fixtureStatus = "created"
    if (Test-Path -LiteralPath $fixturePath) {
        Remove-Item -LiteralPath $temporaryFixturePath
        $fixtureStatus = "unchanged"
    }
    else {
        Move-Item -LiteralPath $temporaryFixturePath -Destination $fixturePath
    }
}
finally {
    if (Test-Path -LiteralPath $temporaryFixturePath) {
        Remove-Item -LiteralPath $temporaryFixturePath
    }
}

$manifest = [ordered]@{
    source = "Polymarket Gamma API"
    docs_url = "https://docs.polymarket.com/api-reference/events/list-events-keyset-pagination"
    verified_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    as_of_utc = $AsOfUtc
    limit_per_scope = $LimitPerScope
    source_responses = @($sourceRecords)
    fixture = [ordered]@{
        local_path = (Get-Item -LiteralPath $fixturePath).FullName
        sha256 = $fixtureHash
        size_bytes = (Get-Item -LiteralPath $fixturePath).Length
        status = $fixtureStatus
        future_candidates = $futureCandidates.Count
        historical_candidates = $historicalCandidates.Count
        open_future_candidates = @($futureCandidates | Where-Object { -not $_.closed -and $_.accepting_orders }).Count
        all_candidates_have_ids = @($allCandidates | Where-Object {
                -not [string]::IsNullOrWhiteSpace($_.event_id) -and
                -not [string]::IsNullOrWhiteSpace($_.market_id) -and
                -not [string]::IsNullOrWhiteSpace($_.condition_id) -and
                $_.clob_token_ids.Count -eq 2
            }).Count -eq $allCandidates.Count
    }
}
$manifestContract = [ordered]@{
    as_of_utc = $AsOfUtc
    limit_per_scope = $LimitPerScope
    future_query_sha256 = $sourceRecords[0].query_sha256
    historical_query_sha256 = $sourceRecords[1].query_sha256
}
$manifestContractJson = $manifestContract | ConvertTo-Json -Compress
$manifestHasher = [System.Security.Cryptography.SHA256]::Create()
try {
    $manifestHashBytes = $manifestHasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($manifestContractJson))
}
finally {
    $manifestHasher.Dispose()
}
$manifestKey = ([System.BitConverter]::ToString($manifestHashBytes) -replace '-', '').ToLowerInvariant()
$manifestPath = Join-Path $manifestDirectory ("lol-market-catalog.{0}.manifest.json" -f $manifestKey.Substring(0, 12))
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    AsOfUtc = $AsOfUtc
    FutureSourceStatus = $sourceRecords[0].source_status
    HistoricalSourceStatus = $sourceRecords[1].source_status
    FutureCandidates = $futureCandidates.Count
    OpenFutureCandidates = $manifest.fixture.open_future_candidates
    HistoricalCandidates = $historicalCandidates.Count
    FixtureStatus = $fixtureStatus
    FixtureSha256 = $fixtureHash
    Manifest = $manifestPath
}
