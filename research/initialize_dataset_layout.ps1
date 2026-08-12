[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$directories = @(
    [pscustomobject]@{ Name = "raw"; RelativePath = "data/raw" },
    [pscustomobject]@{ Name = "processed"; RelativePath = "data/processed" },
    [pscustomobject]@{ Name = "artifacts"; RelativePath = "artifacts" }
)

$results = foreach ($directory in $directories) {
    $absolutePath = Join-Path $repositoryRoot $directory.RelativePath
    New-Item -ItemType Directory -Force -Path $absolutePath | Out-Null

    # 研究数据和模型产物必须留在本地；若 ignore 合同失效就立即停止，避免误提交第三方 raw 或大型 artifact。
    $ignoreProbe = "{0}/.hist-001-ignore-probe" -f $directory.RelativePath
    & git -C $repositoryRoot check-ignore --quiet -- $ignoreProbe
    if ($LASTEXITCODE -ne 0) {
        throw "目录未被 Git 忽略：$($directory.RelativePath)"
    }

    [pscustomobject][ordered]@{
        name = $directory.Name
        relative_path = $directory.RelativePath
        absolute_path = $absolutePath
        exists = Test-Path -LiteralPath $absolutePath -PathType Container
        git_ignored = $true
    }
}

$results | ConvertTo-Json -Depth 3
