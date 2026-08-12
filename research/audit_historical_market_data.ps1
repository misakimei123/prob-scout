[CmdletBinding()]
param(
    [string]$ReviewCsvPath = "",
    [string]$OutputRoot = "",
    [string]$CoverageCsvPath = "",
    [ValidateRange(1, 1440)]
    [int]$DecisionLeadMinutes = 15,
    [ValidateRange(1, 720)]
    [int]$LookbackHours = 24,
    [ValidateRange(1, 1440)]
    [int]$FidelityMinutes = 1,
    [switch]$Refresh,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Refresh -and $Offline) {
    throw "Refresh 与 Offline 不能同时使用。"
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

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

function Get-HistoryEvidence(
    [string]$TokenId,
    [long]$StartTimestamp,
    [long]$EndTimestamp,
    [string]$OutcomeLabel,
    [string]$MarketId
) {
    # 查询参数本身参与 cache key，避免把其他决策窗口或 fidelity 的响应误当成当前证据。
    $query = [ordered]@{
        market = $TokenId
        startTs = $StartTimestamp
        endTs = $EndTimestamp
        fidelity = $FidelityMinutes
    }
    $queryJson = $query | ConvertTo-Json -Compress
    $queryHash = Get-TextSha256 $queryJson
    $manifestPath = Join-Path $manifestDirectory ("prices-history.{0}.json" -f $queryHash.Substring(0, 16))
    $sourcePath = $null
    $sourceHash = $null
    $sourceStatus = "downloaded"

    if (-not $Refresh -and (Test-Path -LiteralPath $manifestPath)) {
        $cache = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
        if (
            $cache.query_sha256 -eq $queryHash -and
            (Test-Path -LiteralPath $cache.local_path) -and
            (Get-Sha256 $cache.local_path) -eq $cache.sha256
        ) {
            $sourcePath = [string]$cache.local_path
            $sourceHash = [string]$cache.sha256
            $sourceStatus = "cached"
        }
    }

    if ($null -eq $sourcePath -and $Offline) {
        throw "Offline 模式缺少 MarketId=$MarketId Outcome=$OutcomeLabel 的有效 price history cache。"
    }

    if ($null -eq $sourcePath) {
        $url = "https://clob.polymarket.com/prices-history?market={0}&startTs={1}&endTs={2}&fidelity={3}" -f `
            $TokenId, $StartTimestamp, $EndTimestamp, $FidelityMinutes
        $temporaryPath = Join-Path $historyDirectory (".history-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
        try {
            $response = Invoke-WebRequest `
                -UseBasicParsing `
                -Uri $url `
                -Headers @{ "User-Agent" = "prob-scout-research/0.1 (DATA-009 historical coverage)" } `
                -TimeoutSec 30 `
                -OutFile $temporaryPath `
                -PassThru
            if ([int]$response.StatusCode -ne 200 -or [string]$response.Headers["Content-Type"] -notmatch '^application/json') {
                throw "MarketId=$MarketId Outcome=$OutcomeLabel 的 price history 未返回预期 JSON。"
            }

            $sourceHash = Get-Sha256 $temporaryPath
            $sourcePath = Join-Path $historyDirectory ("history.{0}.{1}.json" -f $queryHash.Substring(0, 12), $sourceHash.Substring(0, 12))
            if (Test-Path -LiteralPath $sourcePath) {
                Remove-Item -LiteralPath $temporaryPath
                $sourceStatus = "unchanged"
            }
            else {
                Move-Item -LiteralPath $temporaryPath -Destination $sourcePath
            }

            [ordered]@{
                query_sha256 = $queryHash
                request_url = $url
                captured_at_utc = (Get-Date).ToUniversalTime().ToString("o")
                local_path = $sourcePath
                sha256 = $sourceHash
            } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath $manifestPath
        }
        finally {
            if (Test-Path -LiteralPath $temporaryPath) {
                Remove-Item -LiteralPath $temporaryPath
            }
        }
    }

    $payload = Get-Content -Raw -LiteralPath $sourcePath | ConvertFrom-Json
    if ($null -eq $payload.PSObject.Properties['history']) {
        throw "MarketId=$MarketId Outcome=$OutcomeLabel 的响应缺少 history 字段。"
    }

    $points = @($payload.history)
    foreach ($point in $points) {
        # 决策时点以后或窗口以前的数据会形成未来泄漏；价格越界则说明响应不可用于研究。
        $timestamp = [long]$point.t
        $price = [double]$point.p
        if ($timestamp -lt $StartTimestamp -or $timestamp -gt $EndTimestamp) {
            throw "MarketId=$MarketId Outcome=$OutcomeLabel 出现窗口外 price point：$timestamp。"
        }
        if ($price -lt 0.0 -or $price -gt 1.0) {
            throw "MarketId=$MarketId Outcome=$OutcomeLabel 出现越界价格：$price。"
        }
    }

    $lastPoint = $points | Sort-Object { [long]$_.t } | Select-Object -Last 1
    return [pscustomobject]@{
        point_count = $points.Count
        last_timestamp = if ($null -eq $lastPoint) { $null } else { [long]$lastPoint.t }
        last_price = if ($null -eq $lastPoint) { $null } else { [double]$lastPoint.p }
        source_path = $sourcePath
        source_sha256 = $sourceHash
        source_status = $sourceStatus
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ReviewCsvPath)) {
    $ReviewCsvPath = Join-Path $repositoryRoot "docs/DATA_008_MAPPING_REVIEW.csv"
}
else {
    $ReviewCsvPath = (Resolve-Path -LiteralPath $ReviewCsvPath).Path
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/historical_market_grade"
}
if ([string]::IsNullOrWhiteSpace($CoverageCsvPath)) {
    $CoverageCsvPath = Join-Path $repositoryRoot "docs/DATA_009_HISTORICAL_MARKET_GRADES.csv"
}

$historyDirectory = Join-Path $OutputRoot "history"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $historyDirectory, $manifestDirectory | Out-Null

$reviewRows = @(Import-Csv -LiteralPath $ReviewCsvPath)
if ($reviewRows.Count -ne 50) {
    throw "DATA-009 固定审计样本必须恰好包含 50 场，当前为 $($reviewRows.Count) 场。"
}
if (@($reviewRows | Group-Object market_id | Where-Object { $_.Count -gt 1 }).Count -gt 0) {
    throw "DATA-009 输入存在重复 market ID。"
}

$coverageRows = [System.Collections.Generic.List[object]]::new()
$downloaded = 0
$cached = 0
$unchanged = 0

foreach ($row in $reviewRows) {
    if (
        [string]::IsNullOrWhiteSpace($row.gamma_token_0) -or
        [string]::IsNullOrWhiteSpace($row.gamma_token_1)
    ) {
        throw "MarketId=$($row.market_id) 缺少 outcome token ID。"
    }

    $gameStart = [datetimeoffset]::Parse($row.clob_game_start_utc).ToUniversalTime()
    $decisionTime = $gameStart.AddMinutes(-$DecisionLeadMinutes)
    $windowStart = $decisionTime.AddHours(-$LookbackHours)
    $startTimestamp = $windowStart.ToUnixTimeSeconds()
    $endTimestamp = $decisionTime.ToUnixTimeSeconds()

    $outcome0 = Get-HistoryEvidence `
        -TokenId $row.gamma_token_0 `
        -StartTimestamp $startTimestamp `
        -EndTimestamp $endTimestamp `
        -OutcomeLabel $row.gamma_outcome_0 `
        -MarketId $row.market_id
    $outcome1 = Get-HistoryEvidence `
        -TokenId $row.gamma_token_1 `
        -StartTimestamp $startTimestamp `
        -EndTimestamp $endTimestamp `
        -OutcomeLabel $row.gamma_outcome_1 `
        -MarketId $row.market_id

    foreach ($status in @($outcome0.source_status, $outcome1.source_status)) {
        switch ($status) {
            "downloaded" { $downloaded++ }
            "cached" { $cached++ }
            "unchanged" { $unchanged++ }
            default { throw "未知 source status：$status" }
        }
    }

    $hasPriceHistory = $outcome0.point_count -gt 0 -and $outcome1.point_count -gt 0
    # 官方历史接口没有返回 bid/ask 或 depth；没有决策时点自采快照时只能降级，严禁事后用当前 order book 补证。
    $hasHistoricalBidAsk = $false
    $hasHistoricalDepth = $false
    $grade = if ($hasHistoricalDepth) {
        "A"
    }
    elseif ($hasHistoricalBidAsk) {
        "B"
    }
    elseif ($hasPriceHistory) {
        "C"
    }
    else {
        "Unavailable"
    }

    $reason = switch ($grade) {
        "A" { "decision-time depth plus fee evidence" }
        "B" { "decision-time best bid and ask only" }
        "C" { "official t,p price history for both outcome tokens; no decision-time bid/ask or depth snapshot" }
        default { "one or both outcome tokens have no price point before decision time" }
    }

    $coverageRows.Add([pscustomobject][ordered]@{
        review_id = $row.review_id
        market_id = $row.market_id
        mapping_status = $row.expected_status
        gamma_title = $row.gamma_title
        decision_time_utc = Format-Utc $decisionTime
        history_window_start_utc = Format-Utc $windowStart
        fidelity_minutes = $FidelityMinutes
        outcome_0 = $row.gamma_outcome_0
        outcome_0_point_count = $outcome0.point_count
        outcome_0_last_point_utc = if ($null -eq $outcome0.last_timestamp) { "" } else { Format-Utc ([datetimeoffset]::FromUnixTimeSeconds($outcome0.last_timestamp)) }
        outcome_0_last_price = if ($null -eq $outcome0.last_price) { "" } else { $outcome0.last_price }
        outcome_0_staleness_seconds = if ($null -eq $outcome0.last_timestamp) { "" } else { $endTimestamp - $outcome0.last_timestamp }
        outcome_1 = $row.gamma_outcome_1
        outcome_1_point_count = $outcome1.point_count
        outcome_1_last_point_utc = if ($null -eq $outcome1.last_timestamp) { "" } else { Format-Utc ([datetimeoffset]::FromUnixTimeSeconds($outcome1.last_timestamp)) }
        outcome_1_last_price = if ($null -eq $outcome1.last_price) { "" } else { $outcome1.last_price }
        outcome_1_staleness_seconds = if ($null -eq $outcome1.last_timestamp) { "" } else { $endTimestamp - $outcome1.last_timestamp }
        historical_depth = $hasHistoricalDepth.ToString().ToLowerInvariant()
        historical_bid_ask = $hasHistoricalBidAsk.ToString().ToLowerInvariant()
        price_history = $hasPriceHistory.ToString().ToLowerInvariant()
        grade = $grade
        grade_reason = $reason
        outcome_0_source_sha256 = $outcome0.source_sha256
        outcome_1_source_sha256 = $outcome1.source_sha256
    })
}

$coverageDirectory = Split-Path -Parent $CoverageCsvPath
if (-not [string]::IsNullOrWhiteSpace($coverageDirectory)) {
    New-Item -ItemType Directory -Force -Path $coverageDirectory | Out-Null
}
$coverageRows | Export-Csv -NoTypeInformation -Encoding utf8 -LiteralPath $CoverageCsvPath
$coverageHash = Get-Sha256 $CoverageCsvPath

$gradeGroups = $coverageRows | Group-Object grade | ForEach-Object { @{ $_.Name = $_.Count } }
$gradeCounts = @{}
foreach ($group in $gradeGroups) {
    foreach ($key in $group.Keys) {
        $gradeCounts[$key] = $group[$key]
    }
}

[pscustomobject][ordered]@{
    markets = $coverageRows.Count
    grade_a = if ($gradeCounts.ContainsKey("A")) { $gradeCounts["A"] } else { 0 }
    grade_b = if ($gradeCounts.ContainsKey("B")) { $gradeCounts["B"] } else { 0 }
    grade_c = if ($gradeCounts.ContainsKey("C")) { $gradeCounts["C"] } else { 0 }
    unavailable = if ($gradeCounts.ContainsKey("Unavailable")) { $gradeCounts["Unavailable"] } else { 0 }
    downloaded_responses = $downloaded
    cached_responses = $cached
    unchanged_responses = $unchanged
    coverage_csv = $CoverageCsvPath
    coverage_sha256 = $coverageHash
} | ConvertTo-Json -Depth 4
