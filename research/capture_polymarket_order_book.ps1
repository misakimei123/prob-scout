[CmdletBinding()]
param(
    [string]$CatalogFixturePath = "",
    [string]$MarketId = "",
    [ValidateRange(1, 1000)]
    [decimal]$BudgetU = 10,
    [ValidateRange(0, 240)]
    [int]$PrematchBufferMinutes = 15,
    [string]$OutputRoot = "",
    [switch]$Refresh,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Refresh -and $Offline) {
    throw "Refresh 与 Offline 不能同时使用。"
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($CatalogFixturePath)) {
    $catalogDirectory = Join-Path $repositoryRoot "data/raw/polymarket_gamma/fixture"
    $latestCatalog = Get-ChildItem -LiteralPath $catalogDirectory -Filter "lol-match-winner.*.json" -File |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($null -eq $latestCatalog) {
        throw "未找到 DATA-004 catalog fixture，请先运行 download_polymarket_lol_catalog.ps1。"
    }
    $CatalogFixturePath = $latestCatalog.FullName
}
else {
    $CatalogFixturePath = (Resolve-Path -LiteralPath $CatalogFixturePath).Path
}

$catalog = Get-Content -Raw -LiteralPath $CatalogFixturePath | ConvertFrom-Json
$eligibleCandidates = @($catalog.candidates | Where-Object {
        $_.scope -eq "future" -and
        -not $_.closed -and
        $_.accepting_orders -and
        $_.enable_order_book
    })
if ($eligibleCandidates.Count -eq 0) {
    throw "Catalog fixture 没有开放的 future Match Winner 候选。"
}

if ([string]::IsNullOrWhiteSpace($MarketId)) {
    $candidate = $eligibleCandidates | Select-Object -First 1
}
else {
    $candidate = $eligibleCandidates | Where-Object { $_.market_id -eq $MarketId } | Select-Object -First 1
    if ($null -eq $candidate) {
        throw "MarketId=$MarketId 不在 catalog 的开放 future Match Winner 候选中。"
    }
}

$conditionId = [string]$candidate.condition_id
$outcomes = @($candidate.outcomes)
$tokenIds = @($candidate.clob_token_ids)
if ($outcomes.Count -ne 2 -or $tokenIds.Count -ne 2) {
    throw "候选市场必须包含两个 outcomes 和两个 token IDs。"
}

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/polymarket_clob"
}
$sourceDirectory = Join-Path $OutputRoot "source"
$fixtureDirectory = Join-Path $OutputRoot "fixture"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $sourceDirectory, $fixtureDirectory, $manifestDirectory | Out-Null

$booksEndpoint = "https://clob.polymarket.com/books"
$marketInfoEndpoint = "https://clob.polymarket.com/clob-markets/$conditionId"
$requestBody = @($tokenIds | ForEach-Object { [ordered]@{ token_id = [string]$_ } }) |
    ConvertTo-Json -Depth 3 -Compress
$requestContract = [ordered]@{
    market_id = [string]$candidate.market_id
    condition_id = $conditionId
    token_ids = $tokenIds
    budget_u = $BudgetU.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    prematch_buffer_minutes = $PrematchBufferMinutes
}
$requestContractJson = $requestContract | ConvertTo-Json -Compress
$contractHasher = [System.Security.Cryptography.SHA256]::Create()
try {
    $contractHashBytes = $contractHasher.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($requestContractJson))
}
finally {
    $contractHasher.Dispose()
}
$contractHash = ([System.BitConverter]::ToString($contractHashBytes) -replace '-', '').ToLowerInvariant()
$cachePath = Join-Path $manifestDirectory ("query-cache.{0}.{1}.json" -f $candidate.market_id, $contractHash.Substring(0, 12))

$marketInfoPath = $null
$booksPath = $null
$marketInfoHash = $null
$booksHash = $null
$marketInfoReceivedAtUtc = $null
$quoteRequestStartedAtUtc = $null
$quoteReceivedAtUtc = $null
$marketInfoStatus = "downloaded"
$booksStatus = "downloaded"

# Offline 与普通复跑都只接受 hash 完整的双响应 cache，避免把不同时间的 market info 和 book 拼在一起。
if (-not $Refresh -and (Test-Path -LiteralPath $cachePath)) {
    $cache = Get-Content -Raw -LiteralPath $cachePath | ConvertFrom-Json
    if (
        $cache.contract_sha256 -eq $contractHash -and
        (Test-Path -LiteralPath $cache.market_info.local_path) -and
        (Test-Path -LiteralPath $cache.books.local_path)
    ) {
        $cachedMarketInfoHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cache.market_info.local_path).Hash.ToLowerInvariant()
        $cachedBooksHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cache.books.local_path).Hash.ToLowerInvariant()
        if (
            $cachedMarketInfoHash -eq $cache.market_info.sha256 -and
            $cachedBooksHash -eq $cache.books.sha256
        ) {
            $marketInfoPath = [string]$cache.market_info.local_path
            $booksPath = [string]$cache.books.local_path
            $marketInfoHash = $cachedMarketInfoHash
            $booksHash = $cachedBooksHash
            $marketInfoReceivedAtUtc = if ($cache.market_info.received_at_utc -is [datetime]) {
                $cache.market_info.received_at_utc.ToUniversalTime().ToString("o")
            }
            else {
                [string]$cache.market_info.received_at_utc
            }
            $quoteRequestStartedAtUtc = if ($cache.books.request_started_at_utc -is [datetime]) {
                $cache.books.request_started_at_utc.ToUniversalTime().ToString("o")
            }
            else {
                [string]$cache.books.request_started_at_utc
            }
            $quoteReceivedAtUtc = if ($cache.books.received_at_utc -is [datetime]) {
                $cache.books.received_at_utc.ToUniversalTime().ToString("o")
            }
            else {
                [string]$cache.books.received_at_utc
            }
            $marketInfoStatus = "cached"
            $booksStatus = "cached"
        }
    }
}

if (($null -eq $marketInfoPath -or $null -eq $booksPath) -and $Offline) {
    throw "Offline 模式缺少 MarketId=$($candidate.market_id) 的有效双响应 cache：$cachePath"
}

if ($null -eq $marketInfoPath -or $null -eq $booksPath) {
    $temporaryMarketInfoPath = Join-Path $sourceDirectory (".market-info-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    $temporaryBooksPath = Join-Path $sourceDirectory (".books-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        $marketInfoResponse = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $marketInfoEndpoint `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (Polymarket CLOB snapshot)" } `
            -TimeoutSec 30 `
            -OutFile $temporaryMarketInfoPath `
            -PassThru
        $marketInfoReceivedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
        if (
            [int]$marketInfoResponse.StatusCode -ne 200 -or
            [string]$marketInfoResponse.Headers["Content-Type"] -notmatch '^application/json'
        ) {
            throw "CLOB market info 未返回预期 JSON。"
        }

        $marketInfoHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryMarketInfoPath).Hash.ToLowerInvariant()
        $marketInfoPath = Join-Path $sourceDirectory ("market-info.{0}.{1}.json" -f $candidate.market_id, $marketInfoHash.Substring(0, 12))
        if (Test-Path -LiteralPath $marketInfoPath) {
            Remove-Item -LiteralPath $temporaryMarketInfoPath
            $marketInfoStatus = "unchanged"
        }
        else {
            Move-Item -LiteralPath $temporaryMarketInfoPath -Destination $marketInfoPath
            $marketInfoStatus = "downloaded"
        }

        $marketInfoBeforeQuote = Get-Content -Raw -LiteralPath $marketInfoPath | ConvertFrom-Json
        if ($null -eq $marketInfoBeforeQuote.gst) {
            throw "CLOB market info 缺少 sports game start time (gst)，拒绝盘前采样。"
        }
        $gameStartBeforeQuote = [datetimeoffset]$marketInfoBeforeQuote.gst
        $minimumAllowedStart = [datetimeoffset]::UtcNow.AddMinutes($PrematchBufferMinutes)
        if ($gameStartBeforeQuote.ToUniversalTime() -le $minimumAllowedStart) {
            throw "MarketId=$($candidate.market_id) 已开赛或距离 CLOB gst 不足 $PrematchBufferMinutes 分钟；请显式选择另一个 MarketId。"
        }

        $quoteRequestStartedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
        $booksResponse = Invoke-WebRequest `
            -UseBasicParsing `
            -Method Post `
            -Uri $booksEndpoint `
            -Headers @{ "User-Agent" = "prob-scout-research/0.1 (Polymarket CLOB snapshot)" } `
            -ContentType "application/json" `
            -Body $requestBody `
            -TimeoutSec 30 `
            -OutFile $temporaryBooksPath `
            -PassThru
        $quoteReceivedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
        if (
            [int]$booksResponse.StatusCode -ne 200 -or
            [string]$booksResponse.Headers["Content-Type"] -notmatch '^application/json'
        ) {
            throw "CLOB books 未返回预期 JSON。"
        }

        $booksHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryBooksPath).Hash.ToLowerInvariant()
        $booksPath = Join-Path $sourceDirectory ("books.{0}.{1}.json" -f $candidate.market_id, $booksHash.Substring(0, 12))
        if (Test-Path -LiteralPath $booksPath) {
            Remove-Item -LiteralPath $temporaryBooksPath
            $booksStatus = "unchanged"
        }
        else {
            Move-Item -LiteralPath $temporaryBooksPath -Destination $booksPath
            $booksStatus = "downloaded"
        }
    }
    catch {
        throw "Polymarket CLOB 盘前快照失败：$($_.Exception.Message)"
    }
    finally {
        if (Test-Path -LiteralPath $temporaryMarketInfoPath) {
            Remove-Item -LiteralPath $temporaryMarketInfoPath
        }
        if (Test-Path -LiteralPath $temporaryBooksPath) {
            Remove-Item -LiteralPath $temporaryBooksPath
        }
    }
}

try {
    $marketInfo = Get-Content -Raw -LiteralPath $marketInfoPath | ConvertFrom-Json
    $books = @(Get-Content -Raw -LiteralPath $booksPath | ConvertFrom-Json)
}
catch {
    throw "CLOB cache 不是有效 JSON：$($_.Exception.Message)"
}

if ($books.Count -ne 2) {
    throw "CLOB batch books 必须返回双方两个 order books，实际为 $($books.Count)。"
}
if ([string]$marketInfo.c -ne $conditionId) {
    throw "CLOB market info condition ID 与 catalog 不一致。"
}

$marketTokens = @($marketInfo.t)
if ($marketTokens.Count -ne 2) {
    throw "CLOB market info 必须包含两个 tokens。"
}
for ($index = 0; $index -lt 2; $index++) {
    if (
        [string]$marketTokens[$index].t -ne [string]$tokenIds[$index] -or
        [string]$marketTokens[$index].o -ne [string]$outcomes[$index]
    ) {
        throw "CLOB token/outcome 索引映射与 Gamma catalog 不一致。"
    }
}

$gameStart = [datetimeoffset]$marketInfo.gst
$quoteReceived = [datetimeoffset]$quoteReceivedAtUtc
if ($gameStart.ToUniversalTime() -le $quoteReceived.ToUniversalTime().AddMinutes($PrematchBufferMinutes)) {
    throw "快照不满足盘前 buffer：gst=$($gameStart.ToUniversalTime().ToString('o'))，received=$quoteReceivedAtUtc。"
}

$feeRate = [decimal]::Parse([string]$marketInfo.fd.r, [System.Globalization.CultureInfo]::InvariantCulture)
$feeExponent = [int]$marketInfo.fd.e
$takerOnly = [bool]$marketInfo.fd.to
if ($feeExponent -ne 1) {
    throw "当前 DATA-005 只实现官方一阶 fee curve；实际 exponent=$feeExponent，必须交给后续 SDK fill engine。"
}

$analysisRows = [System.Collections.Generic.List[object]]::new()
for ($index = 0; $index -lt 2; $index++) {
    $tokenId = [string]$tokenIds[$index]
    $book = $books | Where-Object { [string]$_.asset_id -eq $tokenId } | Select-Object -First 1
    if ($null -eq $book) {
        throw "CLOB books 缺少 token_id=$tokenId。"
    }
    if ([string]$book.market -ne $conditionId) {
        throw "Token $tokenId 的 book condition ID 与 catalog 不一致。"
    }

    $minOrderSize = [decimal]::Parse([string]$book.min_order_size, [System.Globalization.CultureInfo]::InvariantCulture)
    $tickSize = [decimal]::Parse([string]$book.tick_size, [System.Globalization.CultureInfo]::InvariantCulture)
    if (
        $minOrderSize -ne [decimal]$marketInfo.mos -or
        $tickSize -ne [decimal]$marketInfo.mts
    ) {
        throw "Token $tokenId 的 book 与 market info minimum/tick 不一致。"
    }

    $sortedBids = @($book.bids | Sort-Object { [decimal]::Parse([string]$_.price, [System.Globalization.CultureInfo]::InvariantCulture) } -Descending)
    $sortedAsks = @($book.asks | Sort-Object { [decimal]::Parse([string]$_.price, [System.Globalization.CultureInfo]::InvariantCulture) })
    if ($sortedBids.Count -eq 0 -or $sortedAsks.Count -eq 0) {
        throw "Token $tokenId 缺少 bid 或 ask depth，无法计算可成交样本。"
    }

    $bestBid = [decimal]::Parse([string]$sortedBids[0].price, [System.Globalization.CultureInfo]::InvariantCulture)
    $bestAsk = [decimal]::Parse([string]$sortedAsks[0].price, [System.Globalization.CultureInfo]::InvariantCulture)
    $remainingBudget = $BudgetU
    $filledShares = [decimal]0
    $filledNotional = [decimal]0
    $feeUnrounded = [decimal]0
    $fillLevels = [System.Collections.Generic.List[object]]::new()

    # 10U 是含 taker fee 的总 cash cap；每档按 ask price 与官方 fee curve 共同消耗预算。
    foreach ($ask in $sortedAsks) {
        if ($remainingBudget -le 0) {
            break
        }

        $price = [decimal]::Parse([string]$ask.price, [System.Globalization.CultureInfo]::InvariantCulture)
        $availableShares = [decimal]::Parse([string]$ask.size, [System.Globalization.CultureInfo]::InvariantCulture)
        if ($price -le 0 -or $price -ge 1 -or $availableShares -le 0) {
            throw "Token $tokenId 出现非法 ask level：price=$price size=$availableShares"
        }

        $feePerShare = $feeRate * $price * ([decimal]1 - $price)
        $allInPerShare = $price + $feePerShare
        $sharesByBudget = $remainingBudget / $allInPerShare
        $takenShares = if ($availableShares -lt $sharesByBudget) { $availableShares } else { $sharesByBudget }
        if ($takenShares -le 0) {
            continue
        }

        $levelNotional = $takenShares * $price
        $levelFee = $takenShares * $feePerShare
        $levelCashDebit = $levelNotional + $levelFee
        $filledShares += $takenShares
        $filledNotional += $levelNotional
        $feeUnrounded += $levelFee
        $remainingBudget -= $levelCashDebit
        $fillLevels.Add([pscustomobject][ordered]@{
                price = $price.ToString([System.Globalization.CultureInfo]::InvariantCulture)
                available_shares = $availableShares.ToString([System.Globalization.CultureInfo]::InvariantCulture)
                filled_shares = $takenShares.ToString([System.Globalization.CultureInfo]::InvariantCulture)
                notional_u = $levelNotional.ToString([System.Globalization.CultureInfo]::InvariantCulture)
                fee_u_unrounded = $levelFee.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            })
    }

    if ($filledShares -le 0) {
        throw "Token $tokenId 没有可成交 ask liquidity。"
    }

    $feeRounded = [math]::Round($feeUnrounded, 5, [System.MidpointRounding]::AwayFromZero)
    $cashDebit = $filledNotional + $feeRounded

    # fee 的 5 位小数舍入可能让理论 cash debit 超出预算数个微单位；只回退最后一档的极小 shares。
    $roundingAdjustments = 0
    while ($cashDebit -gt $BudgetU -and $roundingAdjustments -lt 5) {
        $lastFill = $fillLevels[$fillLevels.Count - 1]
        $lastPrice = [decimal]::Parse([string]$lastFill.price, [System.Globalization.CultureInfo]::InvariantCulture)
        $lastShares = [decimal]::Parse([string]$lastFill.filled_shares, [System.Globalization.CultureInfo]::InvariantCulture)
        $lastFeePerShare = $feeRate * $lastPrice * ([decimal]1 - $lastPrice)
        $shareReduction = (($cashDebit - $BudgetU) + [decimal]0.000001) / ($lastPrice + $lastFeePerShare)
        if ($shareReduction -ge $lastShares) {
            throw "Fee rounding 调整会清空最后一档，DATA-005 理论 fill 无法安全计算。"
        }

        $adjustedLastShares = $lastShares - $shareReduction
        $adjustedLastNotional = $adjustedLastShares * $lastPrice
        $adjustedLastFee = $adjustedLastShares * $lastFeePerShare
        $filledShares -= $shareReduction
        $filledNotional -= $shareReduction * $lastPrice
        $feeUnrounded -= $shareReduction * $lastFeePerShare
        $lastFill.filled_shares = $adjustedLastShares.ToString([System.Globalization.CultureInfo]::InvariantCulture)
        $lastFill.notional_u = $adjustedLastNotional.ToString([System.Globalization.CultureInfo]::InvariantCulture)
        $lastFill.fee_u_unrounded = $adjustedLastFee.ToString([System.Globalization.CultureInfo]::InvariantCulture)
        $feeRounded = [math]::Round($feeUnrounded, 5, [System.MidpointRounding]::AwayFromZero)
        $cashDebit = $filledNotional + $feeRounded
        $roundingAdjustments++
    }
    if ($cashDebit -gt $BudgetU) {
        throw "Fee rounding 后 cash debit 仍超过 $BudgetU U。"
    }

    $bookVwap = $filledNotional / $filledShares
    $effectiveEntryPrice = $cashDebit / $filledShares
    $unfilledBudget = $BudgetU - $cashDebit
    if ($unfilledBudget -lt 0) {
        $unfilledBudget = 0
    }
    $fillRatio = $cashDebit / $BudgetU

    $bookTimestampText = [string]$book.timestamp
    $bookTimestampUtc = $null
    $bookAgeAtReceiveMs = $null
    if ($bookTimestampText -match '^\d{13}$') {
        $bookTimestamp = [datetimeoffset]::FromUnixTimeMilliseconds([int64]$bookTimestampText).ToUniversalTime()
        $bookTimestampUtc = $bookTimestamp.ToString("o")
        $bookAgeAtReceiveMs = [math]::Round(($quoteReceived.ToUniversalTime() - $bookTimestamp).TotalMilliseconds)
    }

    $analysisRows.Add([pscustomobject][ordered]@{
            outcome_index = $index
            outcome = [string]$outcomes[$index]
            token_id = $tokenId
            book_timestamp_raw = $bookTimestampText
            book_timestamp_utc = $bookTimestampUtc
            book_age_at_receive_ms = $bookAgeAtReceiveMs
            book_hash = [string]$book.hash
            bid_levels = $sortedBids.Count
            ask_levels = $sortedAsks.Count
            best_bid = $bestBid.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            best_ask = $bestAsk.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            spread = ($bestAsk - $bestBid).ToString([System.Globalization.CultureInfo]::InvariantCulture)
            tick_size = $tickSize.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            min_order_size = $minOrderSize.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            budget_u = $BudgetU.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            filled_shares = $filledShares.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            filled_notional_u = $filledNotional.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            vwap = $bookVwap.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            taker_fee_u = $feeRounded.ToString("0.00000", [System.Globalization.CultureInfo]::InvariantCulture)
            cash_debit_u = $cashDebit.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            effective_entry_price = $effectiveEntryPrice.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            unfilled_budget_u = $unfilledBudget.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            fill_ratio = $fillRatio.ToString([System.Globalization.CultureInfo]::InvariantCulture)
            meets_min_order_size = $filledShares -ge $minOrderSize
            meets_95_percent_fill = $fillRatio -ge [decimal]0.95
            fill_levels = @($fillLevels)
        })
}

$cacheRecord = [ordered]@{
    contract_sha256 = $contractHash
    catalog_fixture_path = $CatalogFixturePath
    market_id = [string]$candidate.market_id
    condition_id = $conditionId
    market_info = [ordered]@{
        endpoint = $marketInfoEndpoint
        received_at_utc = $marketInfoReceivedAtUtc
        local_path = (Get-Item -LiteralPath $marketInfoPath).FullName
        sha256 = $marketInfoHash
        size_bytes = (Get-Item -LiteralPath $marketInfoPath).Length
        status = $marketInfoStatus
    }
    books = [ordered]@{
        endpoint = $booksEndpoint
        request_body = $requestBody | ConvertFrom-Json
        request_started_at_utc = $quoteRequestStartedAtUtc
        received_at_utc = $quoteReceivedAtUtc
        local_path = (Get-Item -LiteralPath $booksPath).FullName
        sha256 = $booksHash
        size_bytes = (Get-Item -LiteralPath $booksPath).Length
        status = $booksStatus
    }
}
$cacheRecord | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $cachePath

$gameStartUtc = $gameStart.ToUniversalTime().ToString("o")
$quoteRequestStarted = [datetimeoffset]$quoteRequestStartedAtUtc
$quoteRequestDurationMs = [math]::Round(($quoteReceived.ToUniversalTime() - $quoteRequestStarted.ToUniversalTime()).TotalMilliseconds)
$fixture = [ordered]@{
    source = "Polymarket CLOB public market data"
    event_id = [string]$candidate.event_id
    event_title = [string]$candidate.event_title
    market_id = [string]$candidate.market_id
    condition_id = $conditionId
    game_start_time_utc = $gameStartUtc
    quote_request_started_at_utc = $quoteRequestStartedAtUtc
    quote_received_at_utc = $quoteReceivedAtUtc
    quote_request_duration_ms = $quoteRequestDurationMs
    prematch_buffer_minutes = $PrematchBufferMinutes
    source_hashes = [ordered]@{
        market_info = $marketInfoHash
        books = $booksHash
    }
    fee = [ordered]@{
        rate = $feeRate.ToString([System.Globalization.CultureInfo]::InvariantCulture)
        exponent = $feeExponent
        taker_only = $takerOnly
        maker_base_fee_bps = [int]$marketInfo.mbf
        taker_base_fee_bps = [int]$marketInfo.tbf
        formula = "shares * rate * price * (1 - price)"
        precision = "estimated fee rounded to 5 decimals"
    }
    outcomes = @($analysisRows)
}

$temporaryFixturePath = Join-Path $fixtureDirectory (".order-book-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
try {
    $fixture | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -LiteralPath $temporaryFixturePath
    $fixtureHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporaryFixturePath).Hash.ToLowerInvariant()
    $fixturePath = Join-Path $fixtureDirectory ("lol-order-book.{0}.{1}.json" -f $candidate.market_id, $fixtureHash.Substring(0, 12))
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
    docs_url = "https://docs.polymarket.com/market-data/prices-order-books"
    verified_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    cache = $cacheRecord
    fixture = [ordered]@{
        local_path = (Get-Item -LiteralPath $fixturePath).FullName
        sha256 = $fixtureHash
        size_bytes = (Get-Item -LiteralPath $fixturePath).Length
        status = $fixtureStatus
        outcome_count = $analysisRows.Count
        both_outcomes_fill_95_percent = @($analysisRows | Where-Object { $_.meets_95_percent_fill }).Count -eq 2
        both_outcomes_meet_min_order_size = @($analysisRows | Where-Object { $_.meets_min_order_size }).Count -eq 2
    }
}
$manifestPath = Join-Path $manifestDirectory ("lol-order-book.{0}.{1}.manifest.json" -f $candidate.market_id, $contractHash.Substring(0, 12))
$manifest | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    MarketId = [string]$candidate.market_id
    ConditionId = $conditionId
    GameStartUtc = $gameStartUtc
    QuoteReceivedAtUtc = $quoteReceivedAtUtc
    MarketInfoStatus = $marketInfoStatus
    BooksStatus = $booksStatus
    FixtureStatus = $fixtureStatus
    FixtureSha256 = $fixtureHash
    Outcome0 = "{0}: ask={1}, vwap={2}, effective={3}" -f $analysisRows[0].outcome, $analysisRows[0].best_ask, $analysisRows[0].vwap, $analysisRows[0].effective_entry_price
    Outcome1 = "{0}: ask={1}, vwap={2}, effective={3}" -f $analysisRows[1].outcome, $analysisRows[1].best_ask, $analysisRows[1].vwap, $analysisRows[1].effective_entry_price
    Manifest = $manifestPath
}
