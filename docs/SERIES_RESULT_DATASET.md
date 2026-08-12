# HIST-003 系列赛结果数据集

更新日期：2026-08-12

范围：只生成每行一场 BO3/BO5 的可追溯赛前身份与最终结果，不计算 HIST-004 特征，不把 Grade C 历史价格解释为可成交报价。

## 1. 数据合同

Rust 合同位于 [`src/series_result.rs`](../src/series_result.rs)，生成器位于 [`research/build_series_result_dataset.ps1`](../research/build_series_result_dataset.ps1)。有效行必须同时满足：

- `series_id` 使用 Leaguepedia `MatchId`，且双方、competition 都是 HIST-002 已审核后可解析的 Canonical ID；
- 只接受完整 BO3/BO5，胜方比分必须分别达到 2/3，负方未达到该阈值；
- `region`、`patch`、Scheduled Start、双方和 winner 均非空；同一 series 的所有逐局 Patch 必须唯一；
- Leaguepedia `MatchSchedule` 的总比分/胜者是 Result Evidence，逐局 `ScoreboardGames` 数量必须等于总比分之和；
- Gamma 必须同时满足 `closed=true`、`umaResolutionStatus=resolved`，两个 `outcomePrices` 必须恰为一组 `0/1`；唯一为 `1` 的 outcome 必须与 series winner 指向同一 Canonical Team。

Leaguepedia 的 [`MatchSchedule`](https://lol.fandom.com/wiki/Module:CargoDeclare/MatchSchedule) schema 明确定义 series `Team1Score`、`Team2Score` 与 `Winner`；[`ScoreboardGames`](https://lol.fandom.com/wiki/Module:CargoDeclare/ScoreboardGames) 提供逐局 `Patch` 与 `MatchId`。身份映射、最终赛果和市场结算是三份独立证据，任何一份成功都不能代替另外两份。

## 2. 确定性重复处理

- 唯一键：`series_id`。
- 同一键的候选按 `result_evidence_id`、`market_resolution_evidence_id`、`market_id` 字典序选择主记录。
- competition、region、Patch、时间、BO、双方、比分、winner、mapping evidence 或 market winner 任一冲突，整组 fail closed。
- 核心事实完全一致时才合并，并将输入数量写入 `duplicate_candidate_count`；输入顺序不影响结果。

Rust 测试覆盖了输入逆序仍选择相同主证据，以及相同 `series_id` 但比分冲突时拒绝生成。当前真实批次的 23 个 `series_id` 全部唯一，因此 `duplicate_candidate_count=1`。

## 3. 本次真实构建

命令：

```powershell
./research/build_series_result_dataset.ps1 -Version "2026-08-12.dee62ca.hist003"
```

固定 DATA-008 50 场审核基线的处理结果：

| 项目 | 数量 |
|---|---:|
| DATA-008 总记录 | 50 |
| `Matched` | 29 |
| 排除 `NeedsReview` | 21 |
| 从 `Matched` 排除 BO1 | 6 |
| 最终 BO3 | 21 |
| 最终 BO5 | 2 |
| 最终 series rows | 23 |
| 缺失必填字段 | 0 |
| winner/resolution 冲突 | 0 |
| 重复候选 | 0 |

本地产物：

- dataset：`data/processed/lol-series-results/2026-08-12.dee62ca.hist003/series-results.csv`
- dataset SHA-256：`04ba36d93f8560d9d0ece628cc372ebcebac58f70e71e7674eb37fb25db9bf95`
- manifest：同目录 `series-results.csv.manifest.json`
- manifest SHA-256：`445acdaba9e41057f457e52327c3a448578d22349325d58f4dc70b146ab83270`

`data/` 按 HIST-001 合同保持 Git ignored；manifest 记录 1 份 DATA-008 immutable snapshot、1 份 Leaguepedia Cargo 响应和 23 份 Gamma resolution 响应的相对路径与 SHA-256。`validate_dataset_manifest` binary 会先严格反序列化，再调用 `DatasetManifest::validate()`。

## 4. 复现与边界

- 默认复用已校验 JSON raw；`-Refresh` 会保存新 hash 文件，不原地覆盖旧 raw。
- processed version 已存在时拒绝覆盖，必须指定新 `-Version`。
- 自动下游仍只消费 DATA-008 `Matched`；21 个 `NeedsReview` 没有因赛果已经出现而自动转为已映射。
- 本数据集只证明 23 场样本的字段与双来源胜者一致，不满足 M2 Gate 的至少 500 场规模，也没有生成任何赛前统计特征。
