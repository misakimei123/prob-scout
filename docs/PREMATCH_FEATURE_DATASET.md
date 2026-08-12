# HIST-004 赛前特征快照

更新日期：2026-08-12

范围：为 HIST-003 的 BO3/BO5 目标赛事生成固定 `T-15m` 的基础 team form 特征；目标输入只包含赛前字段，比分、winner、Result Evidence 和 Market Resolution Evidence 不进入目标合同。

## 1. 防未来泄漏合同

Rust 合同位于 [`src/prematch_features.rs`](../src/prematch_features.rs)，真实构建入口位于 [`research/build_prematch_feature_dataset.ps1`](../research/build_prematch_feature_dataset.ps1)。

- `PrematchSeriesTarget` 只接收 series identity、competition、region、Patch、Scheduled Start、BO、双方 Canonical Team ID 与 Leaguepedia source key；Serde 会拒绝 `winner_team_id` 等未知赛后字段。
- 每个目标的 `snapshot_at_utc = scheduled_start_utc - 15 minutes`。
- 历史 series 结果只有在 `completed_at_utc <= snapshot_at_utc` 时才可参与特征；`completed_at_utc` 使用最后一局 `DateTime_UTC + Gamelength_Number`。
- 向输入追加目标自身或任意 cutoff 之后的结果时，早期快照必须字节语义等价；定向测试对此做完整结构比较。
- Leaguepedia [`ScoreboardGames`](https://lol.fandom.com/wiki/Module%3ACargoDeclare/ScoreboardGames) 提供逐局 `DateTime_UTC` 与 `Gamelength_Number`；[`MatchSchedule`](https://lol.fandom.com/wiki/Module%3ACargoDeclare/MatchSchedule) 的 `DateTime_UTC` 是系列赛计划开始时间，两者不互相替代。

历史 form 按目标行中的 Leaguepedia **精确 source key** 统计，不把 2026-08 审核时点的 Canonical Team identity 向前外推。名称变化、缩写或旧名不会被 fuzzy merge，只会减少历史覆盖；需要跨名称合并时，必须先补带有效区间的 HIST-002 identity evidence。

## 2. 基础特征

每一方输出以下字段，并保持双方在目标 series 中的原始顺序：

| 特征 | 值 | 来源时间 |
|---|---|---|
| `prior_series_count` | cutoff 前完整 BO3/BO5 数量 | 最新 eligible series 完成时间 |
| `prior_series_win_rate` | 精确 `wins / series_count` | 同上 |
| `prior_game_count` | 历史系列赛内完整小局数量 | 同上 |
| `prior_game_win_rate` | 精确 `game_wins / game_count` | 同上 |
| `same_patch_series_count` | 与目标 Patch 完全相同的历史 series 数 | 最新同 Patch series 完成时间 |
| `same_patch_series_win_rate` | 精确 `wins / same_patch_count` | 同上 |
| `rest_minutes` | cutoff 距最近 series 完成时间的整分钟数 | 最近 series 完成时间 |

比率保存整数分子和分母，不在数据集阶段引入浮点舍入。没有历史时 count/denominator 为 `0`，`rest_minutes.value` 与所有 `source_latest_at_utc` 为 `null`；不使用任意常数填充缺失值。

## 3. Lineage

HIST-004 直接消费 HIST-003 processed dataset，因此 Dataset Manifest v1 增加可选 `upstream_datasets`：同时固定上游 manifest 路径/hash 与 output 路径/hash。真实历史查询的 4 个 immutable Leaguepedia JSON page 继续记录在 `raw_inputs`。

构建命令：

```powershell
./research/build_prematch_feature_dataset.ps1 `
  -Version "2026-08-12.e678afb.hist004-v2" `
  -SnapshotLeadMinutes 15 `
  -HistoryDays 180
```

## 4. 真实构建结果

| 项目 | 数量 |
|---|---:|
| HIST-003 目标 series | 23 |
| Leaguepedia history rows | 1,761 |
| immutable raw pages | 4 |
| 可用 team observations | 855 |
| fail-closed 排除的不完整 series | 16 |
| 输出快照 | 23 |
| 至少一方有历史的快照 | 23 |
| 晚于 cutoff 的来源时间 | 0 |

- dataset：`data/processed/lol-prematch-features/2026-08-12.e678afb.hist004-v2/prematch-feature-snapshots.json`
- dataset SHA-256：`f13e74dd8c3b28d888075ad4fb6ac4616aa34c6c62049e7f4db323e31a76a2fb`
- manifest SHA-256：`60e6de36dac14b349e6a5fa6c07dd13b7cf0f63f62ffddaa38d55171ba7b50ba`
- upstream HIST-003 manifest SHA-256：`445acdaba9e41057f457e52327c3a448578d22349325d58f4dc70b146ab83270`

## 5. 证据边界

- 本任务证明生成器按来源完成时间执行 cutoff，并且目标赛后记录不能改变早期特征；它不证明 Leaguepedia 当前页面等价于比赛当时保存的不可变页面版本。
- 精确 source key 的 rename 召回率可能偏低，但不会猜测跨名称身份；覆盖问题留给 HIST-006 报告。
- 当前仍只有 23 个目标 series，不满足 M2 Gate 的至少 500 场要求；HIST-005 只能先验证时间划分合同，不能把小样本包装成模型有效性证据。
