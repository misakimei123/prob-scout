[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$IdentityManifest,
    [string]$Version = "",
    [string]$OutputRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, (Resolve-Path -LiteralPath $Path).Path)).Replace('\', '/')
}

function Format-Utc([datetimeoffset]$Value) {
    return $Value.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'")
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $repositoryRoot "data"
}
else {
    $OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
}
$IdentityManifest = (Resolve-Path -LiteralPath $IdentityManifest).Path
& cargo run --quiet --locked --bin validate_dataset_manifest -- $IdentityManifest
if ($LASTEXITCODE -ne 0) {
    throw "HIST-010 identity manifest Rust 校验失败。"
}
$identityManifestDocument = Get-Content -Raw -LiteralPath $IdentityManifest | ConvertFrom-Json
if ([string]$identityManifestDocument.dataset.name -ne "lol-historical-identity-evidence") {
    throw "IdentityManifest 不是 HIST-010 identity evidence dataset。"
}
$identityAudit = Join-Path $repositoryRoot ([string]$identityManifestDocument.output.relative_path)
if ((Get-Sha256 $identityAudit) -ne [string]$identityManifestDocument.output.sha256) {
    throw "HIST-010 identity output hash 与 manifest 不一致。"
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40}$') {
    throw "无法读取生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.hist010-series" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}
$processedDirectory = Join-Path $OutputRoot "processed/lol-series-results/$Version"
if (Test-Path -LiteralPath $processedDirectory) {
    throw "processed version 已存在，禁止覆盖：$processedDirectory"
}

$temporaryOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist010-series-{0}.csv" -f [guid]::NewGuid().ToString("N"))
$replayOutput = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-hist010-series-replay-{0}.csv" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($output in @($temporaryOutput, $replayOutput)) {
        & cargo run --quiet --locked --bin write_historical_series_results -- `
            --identity-audit $identityAudit `
            --output $output
        if ($LASTEXITCODE -ne 0) {
            throw "HIST-010 Series Result CSV 构建失败。"
        }
    }
    if ((Get-Sha256 $temporaryOutput) -ne (Get-Sha256 $replayOutput)) {
        throw "HIST-010 Series Result 相同输入双重构建不一致。"
    }
    $rows = @(Import-Csv -LiteralPath $temporaryOutput)
    $identityAuditDocument = Get-Content -Raw -LiteralPath $identityAudit | ConvertFrom-Json
    if ($rows.Count -ne [int]$identityAuditDocument.summary.series_result_count -or $rows.Count -eq 0) {
        throw "HIST-010 Series Result 行数与 identity audit 不一致。"
    }
    if (@($rows.series_id | Sort-Object -Unique).Count -ne $rows.Count) {
        throw "HIST-010 Series Result 包含重复 series_id。"
    }
    New-Item -ItemType Directory -Path $processedDirectory | Out-Null
    $datasetPath = Join-Path $processedDirectory "series-results.csv"
    Move-Item -LiteralPath $temporaryOutput -Destination $datasetPath
}
finally {
    foreach ($temporaryPath in @($temporaryOutput, $replayOutput)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$datasetHash = Get-Sha256 $datasetPath
$rows = @(Import-Csv -LiteralPath $datasetPath)
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
$eventTimes = @($rows | ForEach-Object { [datetimeoffset]$_.scheduled_start_utc } | Sort-Object)
$manifest = [ordered]@{
    manifest_version = 1
    dataset = [ordered]@{ name = "lol-series-results"; version = $Version }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{ git_commit = $gitCommit; dirty = $dirty; diff_sha256 = $diffHash }
    generator = [ordered]@{
        entrypoint = "research/build_expanded_series_result_dataset.ps1"
        arguments = @("-IdentityManifest", (Get-RepositoryRelativePath $repositoryRoot $IdentityManifest), "-Version", $Version)
    }
    upstream_datasets = @([ordered]@{
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $IdentityManifest
            manifest_sha256 = Get-Sha256 $IdentityManifest
            output_relative_path = [string]$identityManifestDocument.output.relative_path
            output_sha256 = [string]$identityManifestDocument.output.sha256
        })
    raw_inputs = @()
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $datasetPath
        sha256 = $datasetHash
        row_count = $rows.Count
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
    throw "HIST-010 Series Result Dataset Manifest v1 校验失败。"
}

[pscustomobject]@{
    Version = $Version
    SeriesRows = $rows.Count
    DistinctPatches = @($rows.patch | Sort-Object -Unique).Count
    DistinctRegions = @($rows.region | Sort-Object -Unique).Count
    DeterministicReplay = $true
    DatasetSha256 = $datasetHash
    Dataset = $datasetPath
    Manifest = $manifestPath
}
