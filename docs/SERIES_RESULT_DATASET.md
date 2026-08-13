# HIST-003 / HIST-007 系列赛结果与市场结算关联

更新日期：2026-08-13

范围：生成每行一场 BO3/BO5 的纯 `Series Result`，并将预测市场结算保存为独立、可选的 `Market Resolution Link`。本合同不计算 HIST-004 特征，也不把 Grade C 历史价格解释为可成交报价。

## 1. 纯 Series Result 合同

Rust 合同位于 [`src/series_result.rs`](../src/series_result.rs)，生成器位于 [`research/build_series_result_dataset.ps1`](../research/build_series_result_dataset.ps1)。有效行必须满足：

- `series_id` 使用 Leaguepedia `MatchId`，双方与 competition 都是 HIST-002 可解析的 Canonical ID；
- 只接受完整 BO3/BO5，胜方比分必须分别达到 2/3，负方未达到该阈值；
- `region`、Patch、Scheduled Start、双方和 winner 均非空，同一 series 的逐局 Patch 必须唯一；
- Leaguepedia `MatchSchedule` 的总比分/胜者是 Result Evidence，`ScoreboardGames` 数量必须等于总比分之和；
- `SeriesResult` schema 不包含 `market_id`、market winner 或 resolution evidence。没有预测市场的可靠赛事结果仍是合法训练语料。

Leaguepedia [`MatchSchedule`](https://lol.fandom.com/wiki/Module:CargoDeclare/MatchSchedule) 提供 series 比分与 winner，[`ScoreboardGames`](https://lol.fandom.com/wiki/Module:CargoDeclare/ScoreboardGames) 提供逐局 Patch 与 `MatchId`。身份映射和最终赛果是两份独立证据，任何一份成功都不能替代另一份。

## 2. 可选 Market Resolution Link

`MarketResolutionLink` 使用 `(series_id, market_id)` 作为业务键。只有实际存在 market mapping 时才构建；缺少 link 不会反向淘汰 Series Result。

存在 link 时必须全部满足：

- Gamma market 身份与已审核 mapping 一致；
- `closed=true` 且 `resolution_status=resolved`；
- 两个 outcome 保持 Gamma/CLOB 原始 index，且恰好对应 series 双方 Canonical Team；
- `outcomePrices` 恰为一组 `0/1`，`winner_outcome_index` 指向唯一的 `1`；
- 市场 winner 与 Series Result winner 一致。

任一条件失败都拒绝该 link，不把错误或未完成结算降级为空 link。只有确实没有 market candidate 的 series 才走 marketless 路径。

消费边界：

| 数据 | 允许用途 | 禁止用途 |
|---|---|---|
| 纯 Series Result | Constant、Elo、统计模型、赛前特征 | Market Baseline、Edge Strategy、PnL |
| Series Result + Market Resolution Link | 市场 outcome label 校验、linked 子集研究 | 用 Grade C price history 证明可成交性 |

## 3. 确定性重复处理

- Series Result 唯一键为 `series_id`；相同赛事事实才可合并，按 `result_evidence_id`、`mapping_evidence_id` 稳定选择主证据。
- Market Resolution Link 唯一键为 `(series_id, market_id)`；相同结算事实才可合并，按 resolution/mapping evidence 稳定选择主证据。
- competition、region、Patch、时间、BO、双方、比分或 winner 冲突时，整组 Series Result fail closed。
- closed/resolved、outcome 顺序、0/1 price 或 winner 冲突时，整组 market link fail closed。
- market link 引用未知 `series_id` 时拒绝；不允许孤儿 link。

## 4. 构建模式与 lineage

只构建纯赛事结果：

```powershell
./research/build_series_result_dataset.ps1 `
  -Version "2026-08-13.8db1666.hist007-marketless" `
  -SkipMarketResolution
```

同时构建当前可用的 market-linked 子集：

```powershell
./research/build_series_result_dataset.ps1 `
  -Version "2026-08-13.8db1666.hist007-linked"
```

两种模式都生成：

- `data/processed/lol-series-results/<version>/series-results.csv`
- 同目录 Dataset Manifest v1，仅记录纯结果实际依赖的 review snapshot 与 Leaguepedia raw。

linked 模式额外生成：

- `data/processed/lol-market-resolution-links/<version>/market-resolution-links.csv`
- 同目录 manifest；通过 `upstream_datasets` 固定纯结果 manifest/output，并在 `raw_inputs` 单独记录 DATA-008 mapping snapshot 与 Gamma resolution。

因此 Gamma raw 缺失或 marketless 构建不会污染纯结果 lineage。

## 5. HIST-007 真实兼容重放

固定 DATA-008 的 23 场 BO3/BO5 用两种模式分别重建：

| 模式 | Series rows | Link rows | Series SHA-256 |
|---|---:|---:|---|
| marketless | 23 | 0 | `336f48a31f313bedce04b499865b7a7bd10657adf7774808cafae1a274ae5a8c` |
| linked | 23 | 23 | `336f48a31f313bedce04b499865b7a7bd10657adf7774808cafae1a274ae5a8c` |

linked dataset SHA-256 为 `cbc49d9ac8e5baedaf337b6ee618fd9d7bbc4df24a07ca73a2e3c1345a6e5946`。两种模式的纯结果 hash 相同，证明市场证据有无不会改变 Series Result。

新纯结果继续重放 HIST-004、HIST-005、HIST-006 后，Feature Snapshot、Temporal Split、Data Quality Report 的 SHA-256 分别保持 `f13e74dd8c3b28d888075ad4fb6ac4616aa34c6c62049e7f4db323e31a76a2fb`、`fefdb5ec783d12d73721f0fe05f71cc6ccfd6aefa56c588d372bc24c84f8cb1d`、`eddd8534144ffdcd9a1ec0a15052395922a7c3675ede12dc768af1982f8a86a2`。

## 6. 边界与下一步

- HIST-007 完成时只证明 market hard dependency 已解除，样本仍未达到 500；该历史 Gate 状态已由下节 HIST-010 扩展结果 supersede。
- 当前真实重放仍使用 DATA-008 的 23 场 fixed baseline，仅用于证明 schema 与下游兼容。
- 下一项工作应基于 Leaguepedia 多时间段、多 Patch 候选扩展 identity/result corpus；身份缺失、歧义、比分/Patch 冲突仍 fail closed。
- marketless series 不得出现在 Market Baseline、Edge Strategy 或历史 PnL 输入中。

## 7. HIST-010 扩展结果

HIST-010 不再从 DATA-008 市场审核批次生成赛事结果，而是消费 `lol-historical-identity-evidence` 中已完成 event-time identity resolution 的候选。`write_historical_series_results` 逐行重新执行 `SeriesResult::validate()`，再输出与原 HIST-003 相同的稳定 CSV grain；manifest 通过 upstream hash 固定 identity audit，`raw_inputs=[]`。

固定版本 `2026-08-13.8db1666.hist010-series` 生成 1,778 条纯 Series Result，覆盖 13 个 Patch、6 个 Region，SHA-256 为 `9e7a1c2d23b13570f16329e733a13457c997826bbde9fcb6fa2ce0c00334ae99`。相同 identity input 双重导出 hash 一致。

这 1,778 场是 marketless 模型语料；只有原先独立校验的 Market Resolution Link 子集可以进入 Market Baseline、Edge Strategy 或 PnL。
