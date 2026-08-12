[CmdletBinding()]
param(
    [string]$FileId = "1v6LRphp2kYciU4SXp0PCjEMuev1bDejc",
    [string]$SourceFileName = "2025_LoL_esports_match_data_from_OraclesElixir.csv",
    [ValidatePattern('^\d{4}-\d{2}-\d{2}$')]
    [string]$StartDate = "2025-01-15",
    [ValidatePattern('^\d{4}-\d{2}-\d{2}$')]
    [string]$EndDate = "2025-01-21",
    [string]$League = "",
    [string]$OutputRoot = "",
    [string]$LocalSourcePath = "",
    [switch]$RefreshSource
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($StartDate -gt $EndDate) {
    throw "StartDate 不能晚于 EndDate。"
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data/raw/oracles_elixir"
}

$sourceDirectory = Join-Path $OutputRoot "source"
$sampleDirectory = Join-Path $OutputRoot "sample"
$manifestDirectory = Join-Path $OutputRoot "manifest"
New-Item -ItemType Directory -Force -Path $sourceDirectory, $sampleDirectory, $manifestDirectory | Out-Null

$officialFolderUrl = "https://drive.google.com/drive/folders/1gLSw0RLjBbtaNy0dgnGQDAZOHIgCe-HH"
$sourceUrl = "https://drive.google.com/file/d/$FileId/view"
$sourceCachePath = Join-Path $manifestDirectory "source-cache.json"
$sourceStatus = "downloaded"
$sourcePath = $null
$sourceHash = $null
$officialSourceSize = $null
$sourceCoverageEndDate = $null

# 浏览器手工下载是官方 Drive 匿名限流时的安全回退；只接受同名 CSV，并复制进 Git 忽略目录。
if (-not [string]::IsNullOrWhiteSpace($LocalSourcePath)) {
    $resolvedLocalSource = (Resolve-Path -LiteralPath $LocalSourcePath).Path
    if ((Split-Path -Leaf $resolvedLocalSource) -ne $SourceFileName) {
        throw "LocalSourcePath 文件名必须与 SourceFileName 一致。"
    }
    $localHeader = Get-Content -LiteralPath $resolvedLocalSource -TotalCount 1
    if ($localHeader -notmatch '(^|,)gameid(,|$)' -or $localHeader -notmatch '(^|,)date(,|$)') {
        throw "LocalSourcePath 缺少 gameid/date CSV 表头。"
    }

    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $resolvedLocalSource).Hash.ToLowerInvariant()
    $sourceStem = [System.IO.Path]::GetFileNameWithoutExtension($SourceFileName)
    $sourcePath = Join-Path $sourceDirectory ("{0}.{1}.csv" -f $sourceStem, $sourceHash.Substring(0, 12))
    if (Test-Path -LiteralPath $sourcePath) {
        $sourceStatus = "unchanged"
    }
    else {
        Copy-Item -LiteralPath $resolvedLocalSource -Destination $sourcePath
        $sourceStatus = "imported"
    }
    $officialSourceSize = (Get-Item -LiteralPath $resolvedLocalSource).Length
    $sourceCoverageEndDate = "complete-file"
}

# 默认复用已经过 hash 校验的本地年度 source，避免每次筛选小窗口都重复消耗上游带宽。
if ($null -eq $sourcePath -and -not $RefreshSource -and (Test-Path -LiteralPath $sourceCachePath)) {
    $cache = Get-Content -Raw -LiteralPath $sourceCachePath | ConvertFrom-Json
    if (
        $cache.file_id -eq $FileId -and
        ($cache.is_complete_source -or $cache.requested_end_date -eq $EndDate) -and
        (Test-Path -LiteralPath $cache.local_path)
    ) {
        $cachedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cache.local_path).Hash.ToLowerInvariant()
        if ($cachedHash -eq $cache.sha256) {
            $sourcePath = $cache.local_path
            $sourceHash = $cachedHash
            $officialSourceSize = [int64]$cache.official_size_bytes
            $sourceCoverageEndDate = [string]$cache.coverage_end_date
            $sourceStatus = "cached"
        }
    }
}

if ($null -eq $sourcePath) {
    $downloadPath = Join-Path $sourceDirectory (".download-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
    try {
        if ($null -eq (Get-Command uvx -ErrorAction SilentlyContinue)) {
            throw "缺少 uvx。请安装 uv，或从官方 Drive 下载后使用 -LocalSourcePath。"
        }

        # Google Drive 大文件确认由成熟的 gdown 处理，不在项目内复制网页协议。
        & uvx --from "gdown==6.1.0" gdown --quiet --output $downloadPath $FileId
        if ($LASTEXITCODE -ne 0) {
            throw "gdown 下载失败。官方 Drive 可能处于配额限制；请从 source_url 浏览器下载后使用 -LocalSourcePath。"
        }

        $header = Get-Content -LiteralPath $downloadPath -TotalCount 1
        if ($header -notmatch '(^|,)gameid(,|$)' -or $header -notmatch '(^|,)date(,|$)') {
            throw "下载结果缺少 gameid/date CSV 表头，拒绝保存。"
        }

        $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $downloadPath).Hash.ToLowerInvariant()
        $sourceStem = [System.IO.Path]::GetFileNameWithoutExtension($SourceFileName)
        $sourcePath = Join-Path $sourceDirectory ("{0}.{1}.csv" -f $sourceStem, $sourceHash.Substring(0, 12))
        $officialSourceSize = (Get-Item -LiteralPath $downloadPath).Length
        $sourceCoverageEndDate = "complete-file"

        if (Test-Path -LiteralPath $sourcePath) {
            # 相同 hash 的源文件已经存在时不覆盖；这就是重复下载的去重判据。
            Remove-Item -LiteralPath $downloadPath
            $sourceStatus = "unchanged"
        }
        else {
            Move-Item -LiteralPath $downloadPath -Destination $sourcePath
            $sourceStatus = "downloaded"
        }
    }
    finally {
        if (Test-Path -LiteralPath $downloadPath) {
            Remove-Item -LiteralPath $downloadPath
        }
    }
}

$sourceFile = Get-Item -LiteralPath $sourcePath
$sourceCache = [ordered]@{
    source_name = "Oracle's Elixir"
    official_folder_url = $officialFolderUrl
    source_url = $sourceUrl
    file_id = $FileId
    source_file_name = $SourceFileName
    local_path = $sourceFile.FullName
    sha256 = $sourceHash
    size_bytes = $sourceFile.Length
    official_size_bytes = $officialSourceSize
    is_complete_source = ($sourceFile.Length -eq $officialSourceSize -and $sourceCoverageEndDate -eq "complete-file")
    requested_end_date = $EndDate
    coverage_end_date = $sourceCoverageEndDate
    verified_at_utc = (Get-Date).ToUniversalTime().ToString("o")
}
$sourceCache | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 -LiteralPath $sourceCachePath

$scopeName = "{0}_{1}" -f $StartDate, $EndDate
if (-not [string]::IsNullOrWhiteSpace($League)) {
    $safeLeague = $League -replace '[^A-Za-z0-9_-]', '_'
    $scopeName = "{0}_{1}" -f $scopeName, $safeLeague
}

$temporarySamplePath = Join-Path $sampleDirectory (".sample-{0}.tmp" -f [guid]::NewGuid().ToString("N"))
$requiredColumns = @("gameid", "date", "league", "position", "teamname", "result", "datacompleteness")
$schemaState = @{ Validated = $false; Columns = @() }
$stats = @{
    SourceRows = 0
    SelectedRows = 0
    InvalidDateRows = 0
    NullCounts = [ordered]@{}
}
foreach ($column in $requiredColumns) {
    $stats.NullCounts[$column] = 0
}
$gameIds = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
$leagues = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)

try {
    # Import-Csv / Export-Csv 以流式 pipeline 处理年度文件；这里只保留指定日期窗口和可选赛区。
    Import-Csv -LiteralPath $sourcePath | ForEach-Object {
        $stats.SourceRows++

        if (-not $schemaState.Validated) {
            $schemaState.Columns = @($_.PSObject.Properties.Name)
            $missingColumns = @($requiredColumns | Where-Object { $_ -notin $schemaState.Columns })
            if ($missingColumns.Count -gt 0) {
                throw "Oracle's Elixir schema 缺少必需字段: $($missingColumns -join ', ')"
            }
            $schemaState.Validated = $true
        }

        $dateText = [string]$_.date
        if ($dateText -notmatch '^\d{4}-\d{2}-\d{2}') {
            $stats.InvalidDateRows++
            return
        }

        $dateKey = $dateText.Substring(0, 10)
        if ($dateKey -lt $StartDate -or $dateKey -gt $EndDate) {
            return
        }
        if (-not [string]::IsNullOrWhiteSpace($League) -and $_.league -ne $League) {
            return
        }

        $stats.SelectedRows++
        [void]$gameIds.Add([string]$_.gameid)
        [void]$leagues.Add([string]$_.league)
        foreach ($column in $requiredColumns) {
            if ([string]::IsNullOrWhiteSpace([string]$_.$column)) {
                $stats.NullCounts[$column]++
            }
        }
        $_
    } | Export-Csv -NoTypeInformation -Encoding utf8 -LiteralPath $temporarySamplePath

    if (-not $schemaState.Validated) {
        throw "Oracle's Elixir 源 CSV 没有数据行。"
    }
    if ($stats.SelectedRows -eq 0) {
        throw "指定窗口没有匹配记录，请调整 StartDate、EndDate 或 League。"
    }

    $sampleHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $temporarySamplePath).Hash.ToLowerInvariant()
    $samplePath = Join-Path $sampleDirectory ("oe_{0}.{1}.csv" -f $scopeName, $sampleHash.Substring(0, 12))
    $sampleStatus = "created"
    if (Test-Path -LiteralPath $samplePath) {
        Remove-Item -LiteralPath $temporarySamplePath
        $sampleStatus = "unchanged"
    }
    else {
        Move-Item -LiteralPath $temporarySamplePath -Destination $samplePath
    }
}
finally {
    if (Test-Path -LiteralPath $temporarySamplePath) {
        Remove-Item -LiteralPath $temporarySamplePath
    }
}

$manifest = [ordered]@{
    source = $sourceCache
    download_status = $sourceStatus
    sample_status = $sampleStatus
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    filter = [ordered]@{
        start_date = $StartDate
        end_date = $EndDate
        league = if ([string]::IsNullOrWhiteSpace($League)) { $null } else { $League }
    }
    sample = [ordered]@{
        local_path = (Get-Item -LiteralPath $samplePath).FullName
        sha256 = $sampleHash
        size_bytes = (Get-Item -LiteralPath $samplePath).Length
        source_rows = $stats.SourceRows
        selected_rows = $stats.SelectedRows
        unique_games = $gameIds.Count
        leagues = @($leagues | Sort-Object)
        column_count = $schemaState.Columns.Count
        columns = $schemaState.Columns
        invalid_source_date_rows = $stats.InvalidDateRows
        selected_null_counts = $stats.NullCounts
    }
}

$manifestPath = Join-Path $manifestDirectory ("oe_{0}.json" -f $scopeName)
$manifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $manifestPath

[pscustomobject]@{
    SourceStatus = $sourceStatus
    SourceSha256 = $sourceHash
    SourceRows = $stats.SourceRows
    SampleStatus = $sampleStatus
    SampleSha256 = $sampleHash
    SampleRows = $stats.SelectedRows
    UniqueGames = $gameIds.Count
    Leagues = (@($leagues | Sort-Object) -join ",")
    Columns = $schemaState.Columns.Count
    Manifest = $manifestPath
}
