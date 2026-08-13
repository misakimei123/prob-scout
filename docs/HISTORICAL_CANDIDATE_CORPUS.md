# HIST-008 Leaguepedia 历史候选语料与覆盖审计

更新日期：2026-08-13

范围：在扩展 Canonical identity 之前，分页采集多个时间段和 Patch 的 Leaguepedia `MatchSchedule` / `ScoreboardGames`，生成结构完整的 BO3/BO5 source-identity 候选以及完整 rejection audit。本任务不生成 Canonical Team/Competition，不构建最终 Series Result、Feature Snapshot、模型或策略。

## 1. 为什么先建立候选层

HIST-007 已解除 Series Result 对 Polymarket 的硬依赖，但原 HIST-003 仍只消费 DATA-008 的 50 场 market mapping。直接把 Leaguepedia 队名转换成 `lol-team:*` 会绕过 HIST-002 的时间化 identity 合同，并错误合并改名、缩写、二队或名称复用。

HIST-008 因此只回答两个问题：

1. 指定半开 UTC 时间范围内，有多少 Leaguepedia series 具有可验证的完整 BO3/BO5 结构？
2. 其余 series 为什么不能进入 identity/result pipeline？

输出候选仍使用 Leaguepedia 原始 `Team1`、`Team2` 和 `OverviewPage` source key。只有后续 identity 任务明确解析双方与 competition 后，它们才可能成为最终 eligible Series Result。

## 2. 采集与不可变边界

入口：[`research/build_historical_candidate_corpus.ps1`](../research/build_historical_candidate_corpus.ps1)。

脚本分别分页查询：

- `MatchSchedule`：MatchId、Scheduled Start、双方、比分、Winner、BestOf、OverviewPage；
- `ScoreboardGames`：MatchId、Patch、局序号、逐局开始时间和局时长。

两条查询不能合成一次 inner join。原因是缺少 `ScoreboardGames` 的 MatchId 仍必须出现在候选宇宙中，并被标记为 `missing_game_rows`，不能静默消失。

每个 Cargo page：

- 使用稳定 `order_by + limit + offset`；
- 以 query hash、offset、response hash 写入 `data/raw/historical_candidates/leaguepedia/`；
- 复用前重新解析 JSON 并核对文件名 hash；
- 不同 offset 返回相同非空 response hash 时 fail closed；
- 达到 `MaxPagesPerQuery` 但没有观察到短页时 fail closed；
- 不回退 HTML scraper。

processed 输出位于：

```text
data/processed/lol-historical-series-candidates/<version>/
├── historical-candidate-audit.json
└── historical-candidate-audit.json.manifest.json
```

Dataset Manifest v1 固定所有 MatchSchedule/ScoreboardGames page 的相对路径、SHA-256 和采集时间，以及生成代码、参数、输出 hash、candidate row count 和 Event 时间范围。

## 3. Candidate 与 rejection 合同

Rust 合同位于 [`src/historical_candidates.rs`](../src/historical_candidates.rs)，构建 binary 位于 [`src/bin/build_historical_candidate_audit.rs`](../src/bin/build_historical_candidate_audit.rs)。

一个 `HistoricalSeriesCandidate` 必须满足：

- Scheduled Start 位于请求的 `[start_utc, end_utc)`；
- MatchId、双方 source key、OverviewPage、比分、Winner、BestOf 均完整；
- 只接受 BO3/BO5，比分表示已完成系列赛且 Winner 一致；
- 双方 source key 不相同；
- ScoreboardGames 存在，局序号从 1 连续且不重复，行数等于最终比分之和；
- 全系列赛 Patch 唯一且无缺失；
- 每局开始时间和局时长有效，最后一局结束时间晚于 Scheduled Start。

候选保存 `scheduled_start_utc` 与实际逐局推导的 `completed_at_utc`，供后续 T-15m 防泄漏使用。它不包含 Canonical ID，也不包含 market 字段。

可预期的不完整业务记录进入 rejection audit，reason 包括：

- `unsupported_best_of`、`invalid_score`、`winner_mismatch`；
- `missing_patch`、`conflicting_patch`；
- `missing_game_rows`、`missing_game_number`、`invalid_game_sequence`、`game_count_mismatch`；
- `missing_required_field`、`duplicate_teams`、`invalid_game_time` 等。

无法定位 MatchId、game 引用范围外 MatchId、source 返回范围外 series、无任何可用 candidate 等结构性合同破坏会终止整个构建。

## 4. 真实构建结果

命令：

```powershell
./research/build_historical_candidate_corpus.ps1 `
  -StartUtc "2025-01-01 00:00:00" `
  -EndUtc "2025-07-01 00:00:00" `
  -Version "2026-08-13.8db1666.hist008-h1-2025"
```

结果：

| 指标 | 数值 |
|---|---:|
| MatchSchedule pages / rows | 20 / 9,935 |
| ScoreboardGames pages / rows | 28 / 13,987 |
| distinct MatchId | 9,935 |
| ready-for-identity candidates | 2,061 |
| rejected series | 7,874 |
| candidate UTC dates | 170 |
| candidate Patch source keys | 13 |
| BO3 / BO5 | 1,617 / 444 |
| unique team source keys | 468 |
| unique competition source keys | 146 |

主要 rejection：

| Reason | 数量 |
|---|---:|
| `unsupported_best_of` | 5,020 |
| `missing_patch` | 2,814 |
| `game_count_mismatch` | 16 |
| `invalid_score` | 13 |
| `missing_required_field` | 5 |
| `completion_not_after_start` | 5 |
| `duplicate_teams` | 1 |

dataset SHA-256：`e80c7dcdff55b5f9c0b92e1669e6e95fdbb1a81a8c35bee339cad7ff7b43daa5`。相同 raw 输入连续构建两次 hash 一致，Dataset Manifest v1 Rust 校验通过。

Leaguepedia Cargo 的 Patch JSON 可能是 number，数字 JSON 本身无法证明被省略的尾随零；HIST-008 因此保存 API 实际渲染的 source text，例如 `25.1` 不会被自行改写成 `25.10`。后续 Patch normalization 必须使用显式权威映射，不能在候选层推断。

## 5. Gate 与下一步

HIST-008 完成不等于已有 2,061 条最终 eligible series。当前只证明候选规模、时间覆盖、Patch source-key 覆盖和结构完整性；468 个 team source key 与 146 个 competition source key 尚未经过时间化 identity resolution。

因此 HIST-008 完成时 M2 Gate 仍为 `NotReadyForM3`，下一任务是 HIST-009 identity coverage audit。该历史状态已由 HIST-009/HIST-010 supersede；当前 identity 与 Gate 结果见 `HISTORICAL_IDENTITY_EVIDENCE.md`。字符串相似度或 slug 自动制造 Canonical ID 的禁令保持不变。
