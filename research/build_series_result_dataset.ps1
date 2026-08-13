[CmdletBinding()]
param(
    [string]$ReviewPath = "",
    [string]$TeamAliasPath = "",
    [string]$CompetitionMappingPath = "",
    [string]$Version = "",
    [string]$OutputRoot = "",
    [switch]$SkipMarketResolution,
    [switch]$Refresh
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Normalize-TeamName([string]$Name) {
    return (($Name.ToLowerInvariant().ToCharArray() | Where-Object { [char]::IsLetterOrDigit($_) }) -join '')
}

function ConvertTo-CanonicalTeamId([string]$Name) {
    $segment = ($Name.ToLowerInvariant() -replace '[^a-z0-9]+', '-').Trim('-')
    if ([string]::IsNullOrWhiteSpace($segment)) {
        throw "无法从已审核 Gamma 队名生成 canonical ID：$Name"
    }
    return "lol-team:$segment"
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

function Save-ImmutableResponse(
    [string]$Uri,
    [string]$Prefix,
    [string]$Directory,
    [string]$UserAgent,
    [switch]$Force
) {
    $cached = Get-ChildItem -LiteralPath $Directory -Filter "$Prefix.*.json" -File -ErrorAction SilentlyContinue |
        Sort-Object Name |
        Select-Object -First 1
    if ($null -ne $cached -and -not $Force) {
        # 复用时仍解析 JSON，避免损坏缓存被当成有效 raw evidence。
        Get-Content -Raw -LiteralPath $cached.FullName | ConvertFrom-Json | Out-Null
        return $cached.FullName
    }

    $temporaryPath = Join-Path $Directory (".download-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        $response = Invoke-WebRequest `
            -UseBasicParsing `
            -Uri $Uri `
            -Headers @{ "User-Agent" = $UserAgent } `
            -TimeoutSec 30 `
            -OutFile $temporaryPath `
            -PassThru
        if ([int]$response.StatusCode -ne 200 -or [string]$response.Headers["Content-Type"] -notmatch '^application/json') {
            throw "上游未返回预期 JSON：HTTP $($response.StatusCode)，Content-Type=$($response.Headers['Content-Type'])"
        }
        Get-Content -Raw -LiteralPath $temporaryPath | ConvertFrom-Json | Out-Null
        $hash = Get-Sha256 $temporaryPath
        $destination = Join-Path $Directory ("{0}.{1}.json" -f $Prefix, $hash.Substring(0, 12))
        if (Test-Path -LiteralPath $destination) {
            Remove-Item -LiteralPath $temporaryPath
        }
        else {
            Move-Item -LiteralPath $temporaryPath -Destination $destination
        }
        return $destination
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

function Get-RepositoryRelativePath([string]$RepositoryRoot, [string]$Path) {
    $relative = [System.IO.Path]::GetRelativePath($RepositoryRoot, (Resolve-Path -LiteralPath $Path).Path)
    return $relative.Replace('\', '/')
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ReviewPath)) {
    $ReviewPath = Join-Path $repositoryRoot "docs/DATA_008_MAPPING_REVIEW.csv"
}
if ([string]::IsNullOrWhiteSpace($TeamAliasPath)) {
    $TeamAliasPath = Join-Path $repositoryRoot "docs/HIST_002_TEAM_ALIAS_REVIEW.csv"
}
if ([string]::IsNullOrWhiteSpace($CompetitionMappingPath)) {
    $CompetitionMappingPath = Join-Path $repositoryRoot "docs/HIST_002_COMPETITION_MAPPING.csv"
}
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist007" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$rawRoot = Join-Path $OutputRoot "raw/series_results"
$reviewDirectory = Join-Path $rawRoot "review"
$leaguepediaDirectory = Join-Path $rawRoot "leaguepedia"
$gammaDirectory = Join-Path $rawRoot "gamma"
$seriesProcessedDirectory = Join-Path $OutputRoot "processed/lol-series-results/$Version"
$marketProcessedDirectory = Join-Path $OutputRoot "processed/lol-market-resolution-links/$Version"
$rawDirectories = @($reviewDirectory, $leaguepediaDirectory)
if (-not $SkipMarketResolution) {
    $rawDirectories += $gammaDirectory
}
New-Item -ItemType Directory -Force -Path $rawDirectories | Out-Null
if (Test-Path -LiteralPath $seriesProcessedDirectory) {
    throw "processed version 已存在，禁止覆盖：$seriesProcessedDirectory"
}
if (-not $SkipMarketResolution -and (Test-Path -LiteralPath $marketProcessedDirectory)) {
    throw "processed version 已存在，禁止覆盖：$marketProcessedDirectory"
}

$reviews = @(Import-Csv -LiteralPath $ReviewPath)
$eligibleReviews = @($reviews | Where-Object {
        $_.expected_status -eq "Matched" -and ($_.best_of -eq "3" -or $_.best_of -eq "5")
    })
if ($reviews.Count -ne 50 -or $eligibleReviews.Count -eq 0) {
    throw "DATA-008 输入不符合固定 50 场合同或没有已解析 BO3/BO5。"
}
if (@($eligibleReviews.leaguepedia_match_id | Sort-Object -Unique).Count -ne $eligibleReviews.Count) {
    throw "DATA-008 的已解析 BO3/BO5 存在重复 Leaguepedia MatchId；必须先人工解决 mapping。"
}

# 将人工审核表复制为 immutable raw snapshot，processed lineage 不直接依赖可编辑 docs 文件。
$reviewHash = Get-Sha256 $ReviewPath
$reviewSnapshot = Join-Path $reviewDirectory ("data-008.{0}.csv" -f $reviewHash.Substring(0, 12))
if (-not (Test-Path -LiteralPath $reviewSnapshot)) {
    Copy-Item -LiteralPath $ReviewPath -Destination $reviewSnapshot
}

$teamAliases = @(Import-Csv -LiteralPath $TeamAliasPath)
$competitionMappings = @(Import-Csv -LiteralPath $CompetitionMappingPath)
if ($teamAliases | Where-Object { $_.review_status -ne "verified_explicit" }) {
    throw "HIST-002 team alias 输入包含未经明确审核的记录。"
}
if ($competitionMappings | Where-Object { $_.review_status -ne "verified_explicit" }) {
    throw "HIST-002 competition 输入包含未经明确审核的记录。"
}

$teamIdsByReviewAndName = @{}
foreach ($alias in $teamAliases) {
    foreach ($reviewId in ([string]$alias.evidence_review_ids).Split(';')) {
        foreach ($name in @([string]$alias.gamma_name, [string]$alias.leaguepedia_name)) {
            $teamIdsByReviewAndName["$reviewId|$name"] = [string]$alias.canonical_team_id
        }
    }
}
$competitionByReview = @{}
foreach ($mapping in $competitionMappings) {
    foreach ($reviewId in ([string]$mapping.evidence_review_ids).Split(';')) {
        if ($competitionByReview.ContainsKey($reviewId) -and $competitionByReview[$reviewId] -ne $mapping.canonical_competition_id) {
            throw "review_id=$reviewId 对应多个 Canonical Competition。"
        }
        $competitionByReview[$reviewId] = [string]$mapping.canonical_competition_id
    }
}

# 一次查询固定时间窗内的 series 与逐局 Patch，随后只消费 DATA-008 中精确 MatchId。
$tables = "MatchSchedule=MS,ScoreboardGames=SG,Tournaments=T"
$fields = "MS.MatchId,MS.DateTime_UTC=MatchStartUtc,MS.Team1,MS.Team2,MS.Team1Score,MS.Team2Score,MS.Winner,MS.BestOf,MS.OverviewPage,T.League,T.Region,SG.Patch,SG.N_GameInMatch"
$where = 'MS.DateTime_UTC >= "2026-08-08 00:00:00" AND MS.DateTime_UTC < "2026-08-12 06:00:00" AND MS.BestOf IN (3,5)'
$parameters = [ordered]@{
    "tables[0]" = $tables
    "fields[0]" = $fields
    "where[0]" = $where
    "join_on[0]" = "MS.MatchId=SG.MatchId,MS.OverviewPage=T.OverviewPage"
    "order_by[0]" = "MS.DateTime_UTC ASC,MS.MatchId ASC,SG.N_GameInMatch ASC"
    "limit[0]" = "500"
    format = "json"
}
$queryPairs = foreach ($entry in $parameters.GetEnumerator()) {
    "{0}={1}" -f [uri]::EscapeDataString($entry.Key), [uri]::EscapeDataString([string]$entry.Value)
}
$leaguepediaUrl = "https://lol.fandom.com/wiki/Special:CargoExport?{0}" -f ($queryPairs -join "&")
$leaguepediaPath = Save-ImmutableResponse `
    -Uri $leaguepediaUrl `
    -Prefix "series-window" `
    -Directory $leaguepediaDirectory `
    -UserAgent "prob-scout-research/0.1 (HIST-003 series results)" `
    -Force:$Refresh
$leaguepediaRows = @(Get-Content -Raw -LiteralPath $leaguepediaPath | ConvertFrom-Json)
if ($leaguepediaRows.Count -eq 0) {
    throw "Leaguepedia 结果窗口为空。"
}

$seriesRawInputs = [System.Collections.Generic.List[object]]::new()
$seriesRawInputs.Add([pscustomobject][ordered]@{
        source = "data_008_review"
        relative_path = Get-RepositoryRelativePath $repositoryRoot $reviewSnapshot
        sha256 = Get-Sha256 $reviewSnapshot
        captured_at_utc = (Get-Item -LiteralPath $reviewSnapshot).LastWriteTimeUtc.ToString("o")
    })
$seriesRawInputs.Add([pscustomobject][ordered]@{
        source = "leaguepedia"
        relative_path = Get-RepositoryRelativePath $repositoryRoot $leaguepediaPath
        sha256 = Get-Sha256 $leaguepediaPath
        captured_at_utc = (Get-Item -LiteralPath $leaguepediaPath).LastWriteTimeUtc.ToString("o")
    })

$candidates = [System.Collections.Generic.List[object]]::new()
$marketCandidates = [System.Collections.Generic.List[object]]::new()
$marketRawInputs = [System.Collections.Generic.List[object]]::new()
if (-not $SkipMarketResolution) {
    # Market Resolution Link 直接依赖人工 market mapping，因此单独记录 review snapshot。
    $marketRawInputs.Add([pscustomobject][ordered]@{
            source = "data_008_review"
            relative_path = Get-RepositoryRelativePath $repositoryRoot $reviewSnapshot
            sha256 = Get-Sha256 $reviewSnapshot
            captured_at_utc = (Get-Item -LiteralPath $reviewSnapshot).LastWriteTimeUtc.ToString("o")
        })
}
foreach ($review in $eligibleReviews) {
    $reviewId = [string]$review.review_id
    $matchRows = @($leaguepediaRows | Where-Object { [string]$_.MatchId -eq [string]$review.leaguepedia_match_id })
    if ($matchRows.Count -eq 0) {
        throw "review_id=$reviewId 的 Leaguepedia MatchId 缺少结果。"
    }
    $first = $matchRows[0]
    foreach ($field in @("MatchStartUtc", "Team1", "Team2", "Team1Score", "Team2Score", "Winner", "BestOf", "OverviewPage", "League", "Region")) {
        if ($field -notin $first.PSObject.Properties.Name -or [string]::IsNullOrWhiteSpace([string]$first.$field)) {
            throw "review_id=$reviewId 的 Leaguepedia 结果缺少 $field。"
        }
    }
    if ([int]$first.BestOf -ne [int]$review.best_of) {
        throw "review_id=$reviewId 的 BO 与已审核 mapping 不一致。"
    }
    $patches = @($matchRows | ForEach-Object { [string]$_.Patch } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object -Unique)
    if ($patches.Count -ne 1 -or @($matchRows | Where-Object { [string]::IsNullOrWhiteSpace([string]$_.Patch) }).Count -gt 0) {
        throw "review_id=$reviewId 的逐局 Patch 缺失或不唯一：$($patches -join ',')"
    }
    $scores = @([int]$first.Team1Score, [int]$first.Team2Score)
    if ($matchRows.Count -ne ($scores[0] + $scores[1])) {
        throw "review_id=$reviewId 的逐局数量与系列赛比分不一致。"
    }
    $winsNeeded = [math]::Floor([int]$first.BestOf / 2) + 1
    $winnerIndex = [int]$first.Winner - 1
    if ($winnerIndex -notin @(0, 1) -or $scores[$winnerIndex] -ne $winsNeeded -or $scores[1 - $winnerIndex] -ge $winsNeeded) {
        throw "review_id=$reviewId 的系列赛比分或 Winner 不表示完整 BO$($first.BestOf)。"
    }

    # 每一方都从当前已审核行解析；基础规范化相同只在同一 review row 内建立精确身份，不跨时段推断。
    $canonicalTeamIds = @()
    for ($index = 0; $index -lt 2; $index++) {
        $gammaName = [string]$review.("gamma_outcome_$index")
        $leaguepediaIndex = if ([int]$review.leaguepedia_team_1_outcome_index -eq $index) { 1 } else { 2 }
        $leaguepediaName = [string]$review.("leaguepedia_team_$leaguepediaIndex")
        $gammaKey = "$reviewId|$gammaName"
        $leaguepediaKey = "$reviewId|$leaguepediaName"
        if ($teamIdsByReviewAndName.ContainsKey($gammaKey)) {
            $canonicalId = [string]$teamIdsByReviewAndName[$gammaKey]
            if (-not $teamIdsByReviewAndName.ContainsKey($leaguepediaKey) -or $teamIdsByReviewAndName[$leaguepediaKey] -ne $canonicalId) {
                throw "review_id=$reviewId 的显式 team alias 未覆盖双方来源名。"
            }
        }
        elseif ((Normalize-TeamName $gammaName) -eq (Normalize-TeamName $leaguepediaName)) {
            $canonicalId = ConvertTo-CanonicalTeamId $gammaName
        }
        else {
            throw "review_id=$reviewId 的 team identity 未 Resolved。"
        }
        $canonicalTeamIds += $canonicalId
    }
    if ($canonicalTeamIds[0] -eq $canonicalTeamIds[1]) {
        throw "review_id=$reviewId 的双方错误解析为同一 Canonical Team。"
    }
    if (-not $competitionByReview.ContainsKey($reviewId)) {
        throw "review_id=$reviewId 的 competition identity 未 Resolved。"
    }

    $leaguepediaWinnerOutcomeIndex = if ($winnerIndex -eq 0) {
        [int]$review.leaguepedia_team_1_outcome_index
    }
    else {
        1 - [int]$review.leaguepedia_team_1_outcome_index
    }
    $leaguepediaTeamIds = @($null, $null)
    $leaguepediaTeamIds[[int]$review.leaguepedia_team_1_outcome_index] = $canonicalTeamIds[[int]$review.leaguepedia_team_1_outcome_index]
    $otherOutcomeIndex = 1 - [int]$review.leaguepedia_team_1_outcome_index
    $leaguepediaTeamIds[$otherOutcomeIndex] = $canonicalTeamIds[$otherOutcomeIndex]
    $leaguepediaWinnerCanonicalId = $canonicalTeamIds[$leaguepediaWinnerOutcomeIndex]

    $candidates.Add([pscustomobject][ordered]@{
            series_id = "leaguepedia:$($review.leaguepedia_match_id)"
            competition_id = [string]$competitionByReview[$reviewId]
            league = [string]$first.League
            region = [string]$first.Region
            patch = [string]$patches[0]
            scheduled_start_utc = Format-Utc ([datetimeoffset]::ParseExact([string]$first.MatchStartUtc, "yyyy-MM-dd HH:mm:ss", [System.Globalization.CultureInfo]::InvariantCulture, [System.Globalization.DateTimeStyles]::AssumeUniversal))
            best_of = [int]$first.BestOf
            team_1_id = $leaguepediaTeamIds[[int]$review.leaguepedia_team_1_outcome_index]
            team_1_name = [string]$first.Team1
            team_2_id = $leaguepediaTeamIds[$otherOutcomeIndex]
            team_2_name = [string]$first.Team2
            team_1_score = $scores[0]
            team_2_score = $scores[1]
            winner_team_id = $leaguepediaWinnerCanonicalId
            mapping_evidence_id = "DATA-008:$reviewId"
            result_evidence_id = "leaguepedia:$($review.leaguepedia_match_id)"
            duplicate_candidate_count = 1
        })

    if (-not $SkipMarketResolution) {
        # 市场证据是可选的独立关联；存在时仍严格验证身份、顺序、结算和赛事胜者。
        $gammaPath = Save-ImmutableResponse `
            -Uri ("https://gamma-api.polymarket.com/markets/{0}" -f $review.market_id) `
            -Prefix ("market.{0}" -f $review.market_id) `
            -Directory $gammaDirectory `
            -UserAgent "prob-scout-research/0.1 (HIST-007 market resolution link)" `
            -Force:$Refresh
        $gamma = Get-Content -Raw -LiteralPath $gammaPath | ConvertFrom-Json
        if ([string]$gamma.id -ne [string]$review.market_id -or [string]$gamma.conditionId -ne [string]$review.condition_id) {
            throw "review_id=$reviewId 的 Gamma resolution 身份与 mapping 不一致。"
        }
        if (-not [bool]$gamma.closed -or [string]$gamma.umaResolutionStatus -ne "resolved") {
            throw "review_id=$reviewId 的 Gamma market 尚未形成最终 resolution。"
        }
        $outcomes = @(([string]$gamma.outcomes | ConvertFrom-Json))
        $outcomePrices = @(([string]$gamma.outcomePrices | ConvertFrom-Json))
        if ($outcomes.Count -ne 2 -or $outcomePrices.Count -ne 2) {
            throw "review_id=$reviewId 的 Gamma market 不是二元 Match Winner。"
        }
        for ($index = 0; $index -lt 2; $index++) {
            if ([string]$outcomes[$index] -ne [string]$review.("gamma_outcome_$index")) {
                throw "review_id=$reviewId 的 Gamma outcome 顺序已漂移。"
            }
        }
        $resolvedWinnerIndexes = @(0..1 | Where-Object { [decimal]$outcomePrices[$_] -eq 1 })
        if ($resolvedWinnerIndexes.Count -ne 1 -or @($outcomePrices | Where-Object { [decimal]$_ -ne 0 -and [decimal]$_ -ne 1 }).Count -gt 0) {
            throw "review_id=$reviewId 的 Gamma outcomePrices 不是唯一 0/1 resolution。"
        }
        $marketWinnerIndex = [int]$resolvedWinnerIndexes[0]
        if ($marketWinnerIndex -ne $leaguepediaWinnerOutcomeIndex) {
            throw "review_id=$reviewId 的 Series Result winner 与 Market Resolution winner 不一致。"
        }

        $gammaHash = Get-Sha256 $gammaPath
        $marketRawInputs.Add([pscustomobject][ordered]@{
                source = "polymarket_gamma"
                relative_path = Get-RepositoryRelativePath $repositoryRoot $gammaPath
                sha256 = $gammaHash
                captured_at_utc = (Get-Item -LiteralPath $gammaPath).LastWriteTimeUtc.ToString("o")
            })
        $marketCandidates.Add([pscustomobject][ordered]@{
                series_id = "leaguepedia:$($review.leaguepedia_match_id)"
                market_id = [string]$review.market_id
                resolution_status = [string]$gamma.umaResolutionStatus
                closed = [bool]$gamma.closed
                outcome_0_team_id = $canonicalTeamIds[0]
                outcome_1_team_id = $canonicalTeamIds[1]
                outcome_0_price = [int][decimal]$outcomePrices[0]
                outcome_1_price = [int][decimal]$outcomePrices[1]
                winner_outcome_index = $marketWinnerIndex
                mapping_evidence_id = "DATA-008:$reviewId"
                resolution_evidence_id = "gamma-market:$($review.market_id):$gammaHash"
                duplicate_candidate_count = 1
            })
    }
}

# 当前输入无重复；仍实现稳定合并规则，未来来源重叠时不会依赖输入顺序。
$results = [System.Collections.Generic.List[object]]::new()
foreach ($group in ($candidates | Group-Object series_id | Sort-Object Name)) {
    $ordered = @($group.Group | Sort-Object result_evidence_id, mapping_evidence_id)
    $primary = $ordered[0]
    $signatureFields = @("competition_id", "league", "region", "patch", "scheduled_start_utc", "best_of", "team_1_id", "team_1_name", "team_2_id", "team_2_name", "team_1_score", "team_2_score", "winner_team_id")
    $primarySignature = (($signatureFields | ForEach-Object { "$($_)=$($primary.$_)" }) -join "`n")
    foreach ($duplicate in ($ordered | Select-Object -Skip 1)) {
        $duplicateSignature = (($signatureFields | ForEach-Object { "$($_)=$($duplicate.$_)" }) -join "`n")
        if ($duplicateSignature -ne $primarySignature) {
            throw "series_id=$($group.Name) 的重复记录存在业务冲突。"
        }
    }
    $primary.duplicate_candidate_count = $ordered.Count
    $results.Add($primary)
}
if ($results.Count -ne $eligibleReviews.Count) {
    throw "输出行数与已解析 BO3/BO5 数量不一致：results=$($results.Count)，eligible=$($eligibleReviews.Count)"
}

$marketLinks = [System.Collections.Generic.List[object]]::new()
if (-not $SkipMarketResolution) {
    # 同一 series/market 可合并多份一致证据；任何结算事实冲突均 fail closed。
    foreach ($group in ($marketCandidates | Group-Object series_id, market_id | Sort-Object Name)) {
        $ordered = @($group.Group | Sort-Object resolution_evidence_id, mapping_evidence_id)
        $primary = $ordered[0]
        $signatureFields = @("resolution_status", "closed", "outcome_0_team_id", "outcome_1_team_id", "outcome_0_price", "outcome_1_price", "winner_outcome_index")
        $primarySignature = (($signatureFields | ForEach-Object { "$($_)=$($primary.$_)" }) -join "`n")
        foreach ($duplicate in ($ordered | Select-Object -Skip 1)) {
            $duplicateSignature = (($signatureFields | ForEach-Object { "$($_)=$($duplicate.$_)" }) -join "`n")
            if ($duplicateSignature -ne $primarySignature) {
                throw "series_id=$($primary.series_id), market_id=$($primary.market_id) 的重复结算存在冲突。"
            }
        }
        $primary.duplicate_candidate_count = $ordered.Count
        $marketLinks.Add($primary)
    }
    if ($marketLinks.Count -ne $results.Count) {
        throw "当前固定审核批次的市场关联数量不完整：links=$($marketLinks.Count)，series=$($results.Count)"
    }
}

New-Item -ItemType Directory -Path $seriesProcessedDirectory | Out-Null
$datasetPath = Join-Path $seriesProcessedDirectory "series-results.csv"
$results | Export-Csv -NoTypeInformation -Encoding utf8 -LiteralPath $datasetPath
$datasetHash = Get-Sha256 $datasetPath
$marketDatasetPath = $null
$marketDatasetHash = $null
if (-not $SkipMarketResolution) {
    New-Item -ItemType Directory -Path $marketProcessedDirectory | Out-Null
    $marketDatasetPath = Join-Path $marketProcessedDirectory "market-resolution-links.csv"
    $marketLinks | Export-Csv -NoTypeInformation -Encoding utf8 -LiteralPath $marketDatasetPath
    $marketDatasetHash = Get-Sha256 $marketDatasetPath
}

# dirty hash 覆盖 tracked diff 和非忽略 untracked 文件摘要；排除用户 IDE 目录和生成后的 ignored data。
$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
    try {
        @(& git -C $repositoryRoot diff --binary HEAD) | Set-Content -Encoding utf8 -LiteralPath $statePath
        $untrackedPaths = @(& git -C $repositoryRoot ls-files --others --exclude-standard) | Where-Object { $_ -notlike '.idea/*' }
        foreach ($path in ($untrackedPaths | Sort-Object)) {
            $absolutePath = Join-Path $repositoryRoot $path
            if (Test-Path -LiteralPath $absolutePath -PathType Leaf) {
                Add-Content -Encoding utf8 -LiteralPath $statePath -Value ("UNTRACKED {0} {1}" -f $path, (Get-Sha256 $absolutePath))
            }
        }
        $diffHash = Get-Sha256 $statePath
    }
    finally {
        if (Test-Path -LiteralPath $statePath) {
            Remove-Item -LiteralPath $statePath
        }
    }
}

$generatedAtUtc = (Get-Date).ToUniversalTime().ToString("o")
$orderedSeriesRawInputs = @($seriesRawInputs | Sort-Object relative_path -Unique)
$eventTimes = @($results | ForEach-Object { [datetimeoffset]$_.scheduled_start_utc } | Sort-Object)
$generatorArguments = @("-Version", $Version)
if ($SkipMarketResolution) {
    $generatorArguments += "-SkipMarketResolution"
}
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-series-results"; version = $Version }
    generated_at_utc = $generatedAtUtc
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_series_result_dataset.ps1"
        arguments = $generatorArguments
    }
    raw_inputs = $orderedSeriesRawInputs
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = $results.Count
        event_time_range_utc = [ordered]@{
            start = Format-Utc $eventTimes[0]
            end = Format-Utc $eventTimes[-1]
        }
    }
}
$manifestPath = "$datasetPath.manifest.json"
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

& cargo run --quiet --locked --bin validate_dataset_manifest -- $manifestPath
if ($LASTEXITCODE -ne 0) {
    throw "Dataset Manifest v1 Rust 校验失败。"
}

$marketManifestPath = $null
if (-not $SkipMarketResolution) {
    $marketManifest = [ordered]@{
        manifest_version = 1
        dataset = [ordered]@{ name = "lol-market-resolution-links"; version = $Version }
        generated_at_utc = $generatedAtUtc
        code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
        generator = [ordered]@{
            entrypoint = "research/build_series_result_dataset.ps1"
            arguments = $generatorArguments
        }
        upstream_datasets = @(
            [ordered]@{
                manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $manifestPath
                manifest_sha256 = Get-Sha256 $manifestPath
                output_relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
                output_sha256 = $datasetHash
            }
        )
        raw_inputs = @($marketRawInputs | Sort-Object relative_path -Unique)
        output = [ordered]@{
            relative_path = Get-RepositoryRelativePath $repositoryRoot $marketDatasetPath
            sha256 = $marketDatasetHash
            row_count = $marketLinks.Count
            event_time_range_utc = [ordered]@{
                start = Format-Utc $eventTimes[0]
                end = Format-Utc $eventTimes[-1]
            }
        }
    }
    $marketManifestPath = "$marketDatasetPath.manifest.json"
    $marketManifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $marketManifestPath

    & cargo run --quiet --locked --bin validate_dataset_manifest -- $marketManifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "Market Resolution Link 的 Dataset Manifest v1 Rust 校验失败。"
    }
}

[pscustomobject]@{
    Version = $Version
    ReviewRows = $reviews.Count
    ResolvedMappings = @($reviews | Where-Object { $_.expected_status -eq "Matched" }).Count
    ExcludedNeedsReview = @($reviews | Where-Object { $_.expected_status -eq "NeedsReview" }).Count
    ExcludedBo1 = @($reviews | Where-Object { $_.expected_status -eq "Matched" -and $_.best_of -eq "1" }).Count
    SeriesRows = $results.Count
    DuplicateCandidates = @($results | Measure-Object duplicate_candidate_count -Sum).Sum - $results.Count
    MarketLinkRows = $marketLinks.Count
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
    MarketLinkDatasetSha256 = $marketDatasetHash
    MarketLinkDataset = $marketDatasetPath
    MarketLinkManifest = $marketManifestPath
}
