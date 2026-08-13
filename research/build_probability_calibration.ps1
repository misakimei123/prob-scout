[CmdletBinding()]
param(
    [string]$ModelArtifactPath = "",
    [string]$ModelArtifactManifestPath = "",
    [string]$Version = "",
    [string]$ArtifactRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Sha256([string]$Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-RepositoryRelativePath([string]$Root, [string]$Path) {
    return ([System.IO.Path]::GetRelativePath($Root, $Path)).Replace('\', '/')
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $ArtifactRoot = Join-Path $repositoryRoot "artifacts"
}
else {
    $ArtifactRoot = [System.IO.Path]::GetFullPath($ArtifactRoot)
}

# MODEL-005 固定消费已经审计的不可变 MODEL-004 v2，避免 latest 指针静默漂移。
if ([string]::IsNullOrWhiteSpace($ModelArtifactPath)) {
    $ModelArtifactPath = Join-Path $repositoryRoot "artifacts/models/statistical-model/2026-08-13.7605cdd.model004-v2/statistical-model-artifact.json"
}
else {
    $ModelArtifactPath = [System.IO.Path]::GetFullPath($ModelArtifactPath)
}
if ([string]::IsNullOrWhiteSpace($ModelArtifactManifestPath)) {
    $ModelArtifactManifestPath = "$ModelArtifactPath.manifest.json"
}
else {
    $ModelArtifactManifestPath = [System.IO.Path]::GetFullPath($ModelArtifactManifestPath)
}
foreach ($path in @($ModelArtifactPath, $ModelArtifactManifestPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "缺少 MODEL-005 上游输入：$path"
    }
}

$gitCommit = (& git -C $repositoryRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $gitCommit -notmatch '^[0-9a-f]{40,64}$') {
    throw "无法读取 MODEL-005 生成时 Git commit。"
}
if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = "{0}.{1}.model005" -f (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd"), $gitCommit.Substring(0, 7)
}
if ($Version -notmatch '^[A-Za-z0-9._-]+$') {
    throw "Version 只能包含 ASCII 字母、数字、点、下划线和连字符。"
}

$artifactDirectory = Join-Path $ArtifactRoot "models/probability-calibration/$Version"
if (Test-Path -LiteralPath $artifactDirectory) {
    throw "artifact version 已存在，禁止覆盖：$artifactDirectory"
}
$pythonEntrypoint = Join-Path $repositoryRoot "research/model005_probability_calibration.py"
$temporaryArtifactOne = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model005-{0}.json" -f [guid]::NewGuid().ToString("N"))
$temporaryArtifactTwo = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model005-{0}.json" -f [guid]::NewGuid().ToString("N"))
try {
    foreach ($temporaryArtifact in @($temporaryArtifactOne, $temporaryArtifactTwo)) {
        & uv run --project $repositoryRoot --frozen python $pythonEntrypoint `
            --repository-root $repositoryRoot `
            --model-artifact $ModelArtifactPath `
            --model-manifest $ModelArtifactManifestPath `
            --output $temporaryArtifact
        if ($LASTEXITCODE -ne 0) {
            throw "MODEL-005 Python calibration artifact 构建失败。"
        }
    }
    $firstHash = Get-Sha256 $temporaryArtifactOne
    $secondHash = Get-Sha256 $temporaryArtifactTwo
    if ($firstHash -ne $secondHash) {
        throw "MODEL-005 相同输入重放得到不同 artifact：first=$firstHash, second=$secondHash"
    }
    New-Item -ItemType Directory -Path $artifactDirectory | Out-Null
    $artifactPath = Join-Path $artifactDirectory "probability-calibration-artifact.json"
    Move-Item -LiteralPath $temporaryArtifactOne -Destination $artifactPath
}
finally {
    foreach ($temporaryPath in @($temporaryArtifactOne, $temporaryArtifactTwo)) {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath
        }
    }
}

$artifact = Get-Content -Raw -LiteralPath $artifactPath | ConvertFrom-Json
if ([int]$artifact.artifact_schema_version -ne 1 -or
    [string]$artifact.artifact_kind -ne "probability_calibration" -or
    [string]$artifact.calibration.status -ne "fitted_on_public_calibration_split" -or
    [string]$artifact.calibration.config.method -ne "sigmoid" -or
    [string]$artifact.calibration.config.fitting_split -ne "calibration" -or
    [string]$artifact.final_test_evaluation.status -ne "sealed_not_evaluated") {
    throw "MODEL-005 calibration artifact 合同校验失败。"
}
if ($artifact.final_test_evaluation.PSObject.Properties.Name -contains "series_ids") {
    throw "MODEL-005 artifact 不得包含 final test IDs。"
}

$statusLines = @(& git -C $repositoryRoot status --porcelain=v1 --untracked-files=all)
$dirty = $statusLines.Count -gt 0
$diffHash = $null
if ($dirty) {
    $statePath = Join-Path ([System.IO.Path]::GetTempPath()) ("prob-scout-model005-worktree-{0}.txt" -f [guid]::NewGuid().ToString("N"))
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

$artifactHash = Get-Sha256 $artifactPath
$artifactManifest = [ordered]@{
    artifact_manifest_version = 1
    artifact = [ordered]@{
        kind = "probability-calibration"
        name = "sigmoid-calibration"
        version = $Version
    }
    generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    code = [ordered]@{
        git_commit = $gitCommit
        dirty = $dirty
        diff_sha256 = $diffHash
    }
    generator = [ordered]@{
        entrypoint = "research/build_probability_calibration.ps1"
        arguments = @("-Version", $Version)
    }
    inputs = @(
        [ordered]@{
            artifact_relative_path = Get-RepositoryRelativePath $repositoryRoot $ModelArtifactPath
            artifact_sha256 = Get-Sha256 $ModelArtifactPath
            manifest_relative_path = Get-RepositoryRelativePath $repositoryRoot $ModelArtifactManifestPath
            manifest_sha256 = Get-Sha256 $ModelArtifactManifestPath
        }
    )
    output = [ordered]@{
        relative_path = Get-RepositoryRelativePath $repositoryRoot $artifactPath
        sha256 = $artifactHash
    }
}
$artifactManifestPath = "$artifactPath.manifest.json"
$artifactManifest | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $artifactManifestPath

[pscustomobject]@{
    Version = $Version
    Method = [string]$artifact.calibration.config.method
    CalibrationSeries = [int]$artifact.calibration.fit.series_count
    RawBrier = [double]$artifact.calibration_fit_diagnostics.raw.brier_score
    CalibratedBrier = [double]$artifact.calibration_fit_diagnostics.calibrated.brier_score
    RawLogLoss = [double]$artifact.calibration_fit_diagnostics.raw.log_loss
    CalibratedLogLoss = [double]$artifact.calibration_fit_diagnostics.calibrated.log_loss
    FinalTestSeries = [int]$artifact.final_test_evaluation.series_count
    FinalTestStatus = [string]$artifact.final_test_evaluation.status
    DeterministicReplay = ($firstHash -eq $secondHash)
    ArtifactSha256 = $artifactHash
    Artifact = $artifactPath
    Manifest = $artifactManifestPath
}
