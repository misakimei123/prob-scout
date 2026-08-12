# HIST-001 数据目录与 Dataset Manifest v1

更新日期：2026-08-12

本合同只定义本地历史研究数据的目录、不可变边界和 processed dataset lineage，不生成 HIST-002 的队伍身份数据，也不实现模型、策略、WebSocket 或交易能力。

## 1. 目录合同

```text
data/
├── raw/
│   └── <source>/<capture-or-scope>/...
└── processed/
    └── <dataset-name>/<dataset-version>/
        ├── <dataset-file>
        └── <dataset-file>.manifest.json
artifacts/
└── <artifact-kind>/<artifact-name>/<artifact-version>/...
```

- `data/raw/`：官方下载/API 响应及其 source manifest。文件一旦进入后续 lineage 就不可原地修改；来源修订必须保存为新文件和新 hash。
- `data/processed/`：只保存可由 raw 输入和固定生成器重建的数据集。每个数据文件必须有同目录、同文件名前缀的 manifest。
- `artifacts/`：模型、评估报告和其他派生产物。后续 artifact schema 必须引用输入 dataset manifest 的相对路径和 SHA-256；本任务只预留目录，不提前定义模型合同。
- 路径全部使用仓库相对路径和 `/` 分隔符，禁止绝对路径、`..` 和个人机器目录。
- `/data/` 与 `/artifacts/` 已由 `.gitignore` 排除。仓库只提交生成代码、合同、hash 摘要和允许公开的小型评审结果，不提交第三方 raw 或大型产物。

可重复初始化命令：

```powershell
./research/initialize_dataset_layout.ps1
```

脚本幂等创建三个目录，并逐一通过 `git check-ignore` 验证本地数据不会进入 Git；ignore 规则失效时 fail closed。

## 2. Manifest v1

Rust 合同位于 [`src/dataset_manifest.rs`](../src/dataset_manifest.rs)。所有 processed dataset 必须满足以下 JSON 结构并通过 `DatasetManifest::validate()`：

```json
{
  "manifest_version": 1,
  "dataset": {
    "name": "lol-series-results",
    "version": "2026-08-12.0123456"
  },
  "generated_at_utc": "2026-08-12T12:00:00Z",
  "code": {
    "git_commit": "0123456789abcdef0123456789abcdef01234567",
    "dirty": false,
    "diff_sha256": null
  },
  "generator": {
    "entrypoint": "research/build_series_dataset.ps1",
    "arguments": ["-Season", "2025"]
  },
  "upstream_datasets": [
    {
      "manifest_relative_path": "data/processed/lol-series-results/v1/series.csv.manifest.json",
      "manifest_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "output_relative_path": "data/processed/lol-series-results/v1/series.csv",
      "output_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    }
  ],
  "raw_inputs": [
    {
      "source": "oracles_elixir",
      "relative_path": "data/raw/oracles_elixir/source/2025.csv",
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "captured_at_utc": "2026-08-12T11:00:00Z"
    }
  ],
  "output": {
    "relative_path": "data/processed/lol-series-results/2026-08-12.0123456/series.csv",
    "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "row_count": 500,
    "event_time_range_utc": {
      "start": "2025-01-01T00:00:00Z",
      "end": "2025-12-31T23:59:59Z"
    }
  }
}
```

### 必填追溯字段

| 字段 | 合同 |
|---|---|
| `manifest_version` | 当前必须为 `1`；不兼容修改必须提升版本 |
| `dataset.name/version` | 只能使用可安全作为单个目录名的 ASCII 字母、数字、`. _ -` |
| `generated_at_utc` | manifest 生成时的 UTC 时间 |
| `code.git_commit` | 生成代码的完整小写 Git hash，支持 40/64 位 |
| `code.dirty` | 生成时工作区是否有未提交改动 |
| `code.diff_sha256` | dirty 时必须记录生成前完整 diff 的 SHA-256；clean 时必须为 `null` |
| `generator.entrypoint/arguments` | 实际生成入口的仓库相对路径和参数；不得写个人绝对路径 |
| `upstream_datasets[]` | 可选；processed-on-processed 时固定上游 manifest 与 output 的仓库相对路径和 SHA-256 |
| `raw_inputs[]` | 直接消费的 `data/raw/` 文件；逐项保存 source、路径、SHA-256 和采集时间。只有上游 processed dataset 时可以为空 |
| `output` | 必须位于 `data/processed/`，保存内容 SHA-256、正 row count 和 Event UTC 时间范围 |

## 3. 生成顺序

每个后续 dataset builder 必须按以下顺序执行：

1. 读取 raw 文件前验证其 SHA-256；缺失或 hash 不一致立即失败。
2. 只向新的 `<dataset-version>` 目录写临时输出，不覆盖已有版本。
3. 完成字段与时间防泄漏检查后，计算输出 SHA-256、row count 和 Event 时间范围。
4. 读取生成时 Git commit；若工作区 dirty，同时保存完整 diff 的 SHA-256。
5. 最后写 `<dataset-file>.manifest.json`，再调用 `DatasetManifest::validate()`；校验失败的目录不是有效 processed dataset。
6. artifact 使用数据集时记录 dataset manifest 的路径和 hash，从而经 manifest 回溯到所有 raw 输入。

manifest 证明“使用了哪些字节和代码”，不证明业务内容正确。HIST-002 仍需解决队伍身份，HIST-003 仍需核对 series winner 与市场 resolution，HIST-004 仍需单独证明没有未来泄漏。

## 4. Fail-closed 校验

当前 Rust 合同拒绝：

- 未知 manifest 版本、空字段或不安全的数据集目录名；
- 非仓库相对路径、Windows 反斜杠、绝对路径和 `..`；
- 同时缺少 raw input 与 upstream dataset、重复 raw 路径或 raw 路径越出 `data/raw/`；
- 上游 dataset 路径越出 `data/processed/`、manifest 后缀无效、hash 无效或 manifest 重复；
- 非小写 SHA-256、无效 Git hash、dirty 但缺少 diff hash；
- raw 采集时间晚于 dataset 生成时间；
- output 越出 `data/processed/`、row count 为零或 Event 时间范围倒置。

这套合同不把 Grade C 历史价格升级为可成交报价。若 processed dataset 包含 DATA-009 price history，后续报告仍必须标记其 Grade C 边界。
