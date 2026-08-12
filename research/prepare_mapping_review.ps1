[CmdletBinding()]
param(
    [string]$CatalogFixturePath = "",
    [ValidateSet("future", "historical")]
    [string]$Scope = "historical",
    [ValidateRange(50, 50)]
    [int]$ReviewCount = 50,
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$LeaguepediaStartUtc = "2026-08-08 00:00:00",
    [ValidatePattern('^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$')]
    [string]$LeaguepediaEndUtc = "2026-08-12 23:59:59",
    [ValidateRange(0, 1440)]
    [int]$StartToleranceMinutes = 5,
    [string]$OutputRoot = "",
    [switch]$Refresh,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Refresh -and $Offline) {
    throw "Refresh 与 Offline 不能同时使用。"
}
if ($LeaguepediaStartUtc -ge $LeaguepediaEndUtc) {
    throw "LeaguepediaStartUtc 必须早于 LeaguepediaEndUtc。"
}

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-TextSha256([string]$Value) {
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $hasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Value))
        return ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Normalize-TeamName([string]$Name) {
    # 与 Rust DATA-006 合同保持同一边界：只处理大小写、标点和空白，不做 fuzzy match。
    return $Name.ToLowerInvariant() -replace '[^\p{L}\p{Nd}]', ''
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($CatalogFixturePath)) {
    $catalogDirectory = Join-Path $repositoryRoot "data/raw/polymarket_gamma/fixture"
    $latestCatalog = Get-ChildItem -LiteralPath $catalogDirectory -Filter "lol-match-winner.*.json" -File |
        Where-Object {
            $catalog = Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json
            @($catalog.candidates | Where-Object { $_.scope -eq $Scope }).Count -ge $ReviewCount
        } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $latestCatalog) {
        throw "没有包含 $ReviewCount 个 $Scope 候选的 Gamma fixture。"
    }
    $CatalogFixturePath = $latestCatalog.FullName
}
else {
    $CatalogFixturePath = (Resolve-Path -LiteralPath $CatalogFixturePath).Path
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/mapping_review"
}
$leaguepediaDirectory = Join-Path $OutputRoot "leaguepedia"
$clobDirectory = Join-Path $OutputRoot "clob"
$fixtureDirectory = Join-Path $OutputRoot "fixture"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $leaguepediaDirectory, $clobDirectory, $fixtureDirectory, $manifestDirectory | Out-Null

$catalog = Get-Content -Raw -LiteralPath $CatalogFixturePath | ConvertFrom-Json
$markets = @($catalog.candidates | Where-Object { $_.scope -eq $Scope } | Select-Object -First $ReviewCount)
if ($markets.Count -ne $ReviewCount) {
    throw "Gamma fixture 只有 $($markets.Count) 个 $Scope 候选，无法生成 $ReviewCount 场核验。"
}
if (@($markets | Group-Object market_id | Where-Object { $_.Count -gt 1 }).Count -gt 0) {
    throw "核验输入存在重复 market ID。"
}

# 单次 CargoExport 保存固定窗口内的最小赛程证据，不解析 HTML。
$leaguepediaParameters = [ordered]@{
    "tables[0]" = "MatchSchedule=MS"
    "fields[0]" = "MS.MatchId,MS.DateTime_UTC=MatchStartUtc,MS.Team1,MS.Team2,MS.BestOf,MS.OverviewPage"
    "where[0]" = 'MS.DateTime_UTC >= "{0}" AND MS.DateTime_UTC <= "{1}"' -f $LeaguepediaStartUtc, $LeaguepediaEndUtc
    "group_by[0]" = "MS.MatchId"
    "order_by[0]" = "MS.DateTime_UTC ASC"
    "limit[0]" = "500"
    format = "json"
}
$leaguepediaPairs = foreach ($entry in $leaguepediaParameters.GetEnumerator()) {
    "{0}={1}" -f [uri]::EscapeDataString($entry.Key), [uri]::EscapeDataString([string]$entry.Value)
}
$leaguepediaUrl = "https://lol.fandom.com/wiki/Special:CargoExport?{0}" -f ($leaguepediaPairs -join "&")
$leaguepediaQueryHash = Get-TextSha256 ($leaguepediaParameters | ConvertTo-Json -Compress)
$leaguepediaCachePath = Join-Path $manifestDirectory ("leaguepedia-query.{0}.json" -f $leaguepediaQueryHash.Substring(0, 12))
$leaguepediaSourcePath = $null
$leaguepediaSourceHash = $null
$leaguepediaStatus = "downloaded"

if (-not $Refresh -and (Test-Path -LiteralPath $leaguepediaCachePath)) {
    $cache = Get-Content -Raw -LiteralPath $leaguepediaCachePath | ConvertFrom-Json
    if ((Test-Path -LiteralPath $cache.local_path) -and (Get-Sha256 $cache.local_path) -eq $cache.sha256) {
        $leaguepediaSourcePath = [string]$cache.local_path
        $leaguepediaSourceHash = [string]$cache.sha256
        $leaguepediaStatus = "cached"
    }
}
if ($null -eq $leaguepediaSourcePath -and $Offline) {
    throw "Offline 模式缺少有效 Leaguepedia cache：$leaguepediaCachePath"
}
if ($null -eq $leaguepediaSourcePath) {
    $temporaryPath = Join-Path $leaguepediaDirectory (".leaguepedia-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $leaguepediaUrl `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (DATA-008 mapping review)" } `
            -TimeoutSec 45 `
            -OutFile $temporaryPath `
            -PassThru
        if ([int]$response.StatusCode -ne 200 -or [string]$response.Headers["Content-Type"] -notmatch '^application/json') {
            throw "Leaguepedia CargoExport 未返回预期 JSON。"
        }
        $leaguepediaSourceHash = Get-Sha256 $temporaryPath
        $leaguepediaSourcePath = Join-Path $leaguepediaDirectory ("matches.{0}.json" -f $leaguepediaSourceHash.Substring(0, 12))
        if (Test-Path -LiteralPath $leaguepediaSourcePath) {
            Remove-Item -LiteralPath $temporaryPath
            $leaguepediaStatus = "unchanged"
        }
        else {
            Move-Item -LiteralPath $temporaryPath -Destination $leaguepediaSourcePath
        }
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
    [ordered]@{
        query_sha256 = $leaguepediaQueryHash
        request_url = $leaguepediaUrl
        captured_at_utc = (Get-Date).ToUniversalTime().ToString("o")
        local_path = $leaguepediaSourcePath
        sha256 = $leaguepediaSourceHash
    } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath $leaguepediaCachePath
}

$leaguepediaRows = @(Get-Content -Raw -LiteralPath $leaguepediaSourcePath | ConvertFrom-Json)
if ($leaguepediaRows.Count -eq 0) {
    throw "Leaguepedia 固定窗口没有赛程。"
}

$draftRows = [System.Collections.Generic.List[object]]::new()
$clobHashes = [System.Collections.Generic.List[object]]::new()
$clobDownloaded = 0
$clobCached = 0

foreach ($market in $markets) {
    $conditionId = [string]$market.condition_id
    $marketId = [string]$market.market_id
    $clobCachePath = Join-Path $manifestDirectory ("clob-market.{0}.json" -f $marketId)
    $clobSourcePath = $null
    $clobSourceHash = $null

    if (-not $Refresh -and (Test-Path -LiteralPath $clobCachePath)) {
        $cache = Get-Content -Raw -LiteralPath $clobCachePath | ConvertFrom-Json
        if (
            $cache.condition_id -eq $conditionId -and
            (Test-Path -LiteralPath $cache.local_path) -and
            (Get-Sha256 $cache.local_path) -eq $cache.sha256
        ) {
            $clobSourcePath = [string]$cache.local_path
            $clobSourceHash = [string]$cache.sha256
            $clobCached++
        }
    }
    if ($null -eq $clobSourcePath -and $Offline) {
        throw "Offline 模式缺少 MarketId=$marketId 的有效 CLOB metadata cache。"
    }
    if ($null -eq $clobSourcePath) {
        $temporaryPath = Join-Path $clobDirectory (".clob-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
        try {
            $response = Invoke-WebRequest `
                -UseBasicParsing `
                -Uri ("https://clob.polymarket.com/clob-markets/{0}" -f $conditionId) `
                -Headers @{ "User-Agent" = "prob-scout-research/0.1 (DATA-008 mapping review)" } `
                -TimeoutSec 30 `
                -OutFile $temporaryPath `
                -PassThru
            if ([int]$response.StatusCode -ne 200 -or [string]$response.Headers["Content-Type"] -notmatch '^application/json') {
                throw "MarketId=$marketId 的 CLOB metadata 未返回预期 JSON。"
            }
            $clobSourceHash = Get-Sha256 $temporaryPath
            $clobSourcePath = Join-Path $clobDirectory ("market.{0}.{1}.json" -f $marketId, $clobSourceHash.Substring(0, 12))
            if (Test-Path -LiteralPath $clobSourcePath) {
                Remove-Item -LiteralPath $temporaryPath
            }
            else {
                Move-Item -LiteralPath $temporaryPath -Destination $clobSourcePath
            }
        }
        finally {
            if (Test-Path -LiteralPath $temporaryPath) {
                Remove-Item -LiteralPath $temporaryPath
            }
        }
        [ordered]@{
            market_id = $marketId
            condition_id = $conditionId
            captured_at_utc = (Get-Date).ToUniversalTime().ToString("o")
            local_path = $clobSourcePath
            sha256 = $clobSourceHash
        } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath $clobCachePath
        $clobDownloaded++
    }

    $clob = Get-Content -Raw -LiteralPath $clobSourcePath | ConvertFrom-Json
    if ($null -eq $clob.gst) {
        throw "MarketId=$marketId 缺少 CLOB gst。"
    }
    $outcomes = @($market.outcomes)
    $tokenIds = @($market.clob_token_ids)
    $clobTokens = @($clob.t)
    if ($outcomes.Count -ne 2 -or $tokenIds.Count -ne 2 -or $clobTokens.Count -ne 2) {
        throw "MarketId=$marketId 必须恰好包含两个 outcome/token。"
    }
    for ($index = 0; $index -lt 2; $index++) {
        if ([string]$clobTokens[$index].t -ne [string]$tokenIds[$index] -or [string]$clobTokens[$index].o -ne [string]$outcomes[$index]) {
            throw "MarketId=$marketId 的 CLOB token/outcome index 与 Gamma 不一致。"
        }
    }

    $bestOfMatch = [regex]::Match([string]$market.event_title, '\(BO([135])\)')
    if (-not $bestOfMatch.Success) {
        throw "MarketId=$marketId 无法从 Gamma title 解析 BO。"
    }
    $bestOf = [int]$bestOfMatch.Groups[1].Value
    $gameStart = [datetimeoffset]$clob.gst
    $normalizedOutcomes = @(
        Normalize-TeamName ([string]$outcomes[0])
        Normalize-TeamName ([string]$outcomes[1])
    )
    $nearbyRows = @($leaguepediaRows | Where-Object {
            $rowStart = [datetimeoffset]::ParseExact(
                [string]$_.MatchStartUtc,
                "yyyy-MM-dd HH:mm:ss",
                [System.Globalization.CultureInfo]::InvariantCulture,
                [System.Globalization.DateTimeStyles]::AssumeUniversal
            )
            [int]($_.BestOf) -eq $bestOf -and
            [math]::Abs(($rowStart.ToUniversalTime() - $gameStart.ToUniversalTime()).TotalMinutes) -le $StartToleranceMinutes
        })
    $exactRows = @($nearbyRows | Where-Object {
            $rowNames = @(
                Normalize-TeamName ([string]$_.Team1)
                Normalize-TeamName ([string]$_.Team2)
            )
            $rowNames[0] -in $normalizedOutcomes -and
            $rowNames[1] -in $normalizedOutcomes -and
            $rowNames[0] -ne $rowNames[1]
        })

    $proposed = if ($exactRows.Count -eq 1) { $exactRows[0] } else { $null }
    $nearbySummary = @($nearbyRows | ForEach-Object {
            "{0}|{1} vs {2}|BO{3}|{4}" -f $_.MatchId, $_.Team1, $_.Team2, $_.BestOf, $_.MatchStartUtc
        }) -join "; "
    $draftRows.Add([pscustomobject][ordered]@{
            market_id = $marketId
            polymarket_event_id = [string]$market.event_id
            condition_id = $conditionId
            gamma_title = [string]$market.event_title
            gamma_outcome_0 = [string]$outcomes[0]
            gamma_token_0 = [string]$tokenIds[0]
            gamma_outcome_1 = [string]$outcomes[1]
            gamma_token_1 = [string]$tokenIds[1]
            gamma_market_end_utc = Format-Utc ([datetimeoffset]$market.event_end_date_utc)
            clob_game_start_utc = Format-Utc $gameStart
            best_of = $bestOf
            exact_candidate_count = $exactRows.Count
            proposed_leaguepedia_match_id = if ($null -ne $proposed) { [string]$proposed.MatchId } else { "" }
            proposed_leaguepedia_start_utc = if ($null -ne $proposed) { Format-Utc ([datetimeoffset]::ParseExact([string]$proposed.MatchStartUtc, "yyyy-MM-dd HH:mm:ss", [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::AssumeUniversal)) } else { "" }
            proposed_leaguepedia_team_1 = if ($null -ne $proposed) { [string]$proposed.Team1 } else { "" }
            proposed_leaguepedia_team_2 = if ($null -ne $proposed) { [string]$proposed.Team2 } else { "" }
            nearby_candidates = $nearbySummary
        })
    $clobHashes.Add([pscustomobject][ordered]@{
            market_id = $marketId
            sha256 = $clobSourceHash
        })
}

$temporaryDraftPath = Join-Path $fixtureDirectory (".mapping-review-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
$draftRows | Export-Csv -NoTypeInformation -Encoding utf8 -LiteralPath $temporaryDraftPath
$draftHash = Get-Sha256 $temporaryDraftPath
$draftPath = Join-Path $fixtureDirectory ("data-008-draft.{0}.csv" -f $draftHash.Substring(0, 12))
$draftStatus = "created"
if (Test-Path -LiteralPath $draftPath) {
    Remove-Item -LiteralPath $temporaryDraftPath
    $draftStatus = "unchanged"
}
else {
    Move-Item -LiteralPath $temporaryDraftPath -Destination $draftPath
}

$manifest = [ordered]@{
    task = "DATA-008"
    review_count = $ReviewCount
    scope = $Scope
    start_tolerance_minutes = $StartToleranceMinutes
    catalog_fixture = [ordered]@{
        path = $CatalogFixturePath
        sha256 = Get-Sha256 $CatalogFixturePath
    }
    leaguepedia = [ordered]@{
        start_utc = $LeaguepediaStartUtc
        end_utc = $LeaguepediaEndUtc
        query_sha256 = $leaguepediaQueryHash
        response_sha256 = $leaguepediaSourceHash
        rows = $leaguepediaRows.Count
    }
    clob_markets = @($clobHashes)
    draft = [ordered]@{
        path = $draftPath
        sha256 = $draftHash
        rows = $draftRows.Count
        exact_unique_candidates = @($draftRows | Where-Object { $_.exact_candidate_count -eq 1 }).Count
        no_exact_candidate = @($draftRows | Where-Object { $_.exact_candidate_count -eq 0 }).Count
        ambiguous_exact_candidates = @($draftRows | Where-Object { $_.exact_candidate_count -gt 1 }).Count
    }
}
$manifestPath = Join-Path $manifestDirectory ("data-008.{0}.manifest.json" -f $draftHash.Substring(0, 12))
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    ReviewCount = $ReviewCount
    LeaguepediaStatus = $leaguepediaStatus
    LeaguepediaRows = $leaguepediaRows.Count
    ClobDownloaded = $clobDownloaded
    ClobCached = $clobCached
    ExactUniqueCandidates = $manifest.draft.exact_unique_candidates
    NoExactCandidate = $manifest.draft.no_exact_candidate
    AmbiguousExactCandidates = $manifest.draft.ambiguous_exact_candidates
    DraftStatus = $draftStatus
    DraftSha256 = $draftHash
    DraftPath = $draftPath
    ManifestPath = $manifestPath
}
