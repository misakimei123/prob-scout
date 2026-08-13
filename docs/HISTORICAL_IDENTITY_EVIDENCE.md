# HIST-010 2025 时间化 Identity Evidence 与 M2 Gate 复审

更新日期：2026-08-13

本任务用 Leaguepedia Cargo 的显式 source relation 补充 2025H1 identity evidence，并重建 HIST-003–HIST-006。它不使用 fuzzy、slug、字符串包含关系或无证据的 source-key fallback。

## 1. 显式证据链

Team identity 必须同时具备：

1. `TeamRedirects.AllName -> _pageName` 的 exact Cargo relation；
2. HIST-008 `MatchSchedule` candidate 在 `scheduled_start_utc` 对该 `AllName` 的赛事观测。

Competition identity 必须同时具备：

1. `Tournaments.OverviewPage -> League + Region` 的 exact Cargo relation；
2. HIST-008 candidate 在 `scheduled_start_utc` 对该 `OverviewPage` 的赛事观测。

Canonical ID 使用带类型前缀的 source identity SHA-256，例如 `lol-team:lp-<sha256>`。hash 输入是 exact canonical page 或 League brand，不是经过 slug、大小写折叠或 fuzzy 处理的名称。

每条 identity period 只覆盖对应赛事观测的半开区间 `[scheduled_start_utc, scheduled_start_utc + 1s)`。因此当前 Cargo relation 只能与明确 MatchSchedule 观测组合使用，不能在两个赛事之间自动插值。缺失 relation 保持 `Missing`；一对多 relation 将全部候选写入 registry 并保持 `Ambiguous`。

## 2. 可重放构建

入口：

```powershell
./research/build_historical_identity_evidence.ps1 `
  -CandidateManifest data/processed/lol-historical-series-candidates/<version>/historical-candidate-audit.json.manifest.json `
  -Version <new-immutable-version>
```

构建器完整分页保存：

- 12 个 `TeamRedirects` raw page，共 5,779 rows；
- 21 个 `Tournaments` raw page，共 10,421 rows。

每页使用 query hash、offset 和 content hash 命名；分页未正常结束、不同 offset 返回同一非空页面、HTTP/Content-Type 异常或 raw hash 不一致都会停止。相同输入连续两次 Rust 构建必须得到相同输出 hash。

输出 `historical-identity-audit.json` 同时保存 evidence summary、time-bounded registry、逐 series coverage、剩余 review queue 和已校验纯 Series Result。

## 3. 真实 identity coverage

固定版本：`2026-08-13.8db1666.hist010`

| 指标 | 结果 |
|---|---:|
| HIST-008 candidates | 2,061 |
| Team source keys | 468 |
| Team Resolved / Missing / Ambiguous | 370 / 98 / 0 |
| Competition source keys | 146 |
| Competition Resolved / Missing / Ambiguous | 146 / 0 / 0 |
| Fully resolved series | 1,778 |
| Blocked series | 283 |
| Remaining review queue | 98 |

Identity audit SHA-256：`e01d8a1fbcf547db23cff33b285a00a95cd663d42953fffde06069931a70fe50`

未解析项没有被 source key 自身兜底为 Canonical identity。影响最大的剩余 key 包括 `TALON (Hong Kong Team)` 22 场、`eSuba` 19 场、`mCon esports` 17 场、`wangting` 16 场和 `htp eSport Akademie Hannover` 16 场。

## 4. HIST-003–HIST-006 重建

| Dataset | 结果 | SHA-256 |
|---|---:|---|
| Series Result | 1,778 rows；13 Patch；6 Region | `9e7a1c2d23b13570f16329e733a13457c997826bbde9fcb6fa2ce0c00334ae99` |
| T-15m Feature Snapshot | 1,778 rows；1,750 至少一方有历史；0 leakage | `3a29cbfc7a9311b6bf36837da0fc2c24df115175460251bab862c6de89d50ab3` |
| Temporal Split | 325 / 349 / 748 / 356 | `1ff428ae74f1a4a7d32dc033244f0aa74ff6268a818303258a7a96c01d699258` |
| Data Quality Report | `ReadyForM3` | `9a32f02e0e1a348ce01a7603163b8ac55bb14bdfd59975f2d40852cd45b92342` |

HIST-004 将 370 个 exact source key 按 25 个一批查询，15 个 query 共保存 40 个 raw page。跨 batch 的同一 `(MatchId, game number)` 只有事实完全一致才去重，冲突立即 fail closed。最终得到 8,987 个唯一 game rows、4,892 个 team observations；3,556 个 team-side feature source time 均不晚于 `T-15m`。缓存重放得到相同 Feature Snapshot SHA-256。

HIST-005 final test 保持 sealed：356 个 ID 不写入 development manifest，只保存 membership SHA-256 `c5b7295b8363bc62c4cbf8d1c0edc798179fa09ad6634060f5207b1397a39f1d`。

## 5. M2 Gate 结论与边界

M2 Gate 更新为 `ReadyForM3`，依据是预注册的 eligible-series 硬门槛已达到：1,778 / 500。该 Gate 只授权进入 Constant/Elo/统计模型开发，不证明模型有效，也不授权 Market Baseline、交易策略或真钱执行。

仍须保留的反方证据：

- 数据只覆盖 2025H1，尚无跨年度稳健性；
- 1,461 / 3,556 team sides（41.09%）没有 same-Patch history，必须保留 unavailable 语义；
- DATA-009 的 50 个市场仍全部为 Grade C，execution-grade snapshot 缺失 100%；
- 98 个 team source keys、283 个 candidates 仍 fail closed；
- Leaguepedia 当前页面可能被事后修订，manifest 证明使用的字节，不证明页面在比赛时不可变。

因此下一任务可以是 `MODEL-001` Constant Baseline，但 Market Baseline、Edge Strategy、PnL 和执行结论仍受独立 evidence gate 约束。

## 6. M3R-003 跨年度复用说明

本文件记录 HIST-010 在 2025H1 的原始任务证据。后续 M3R-003 将同一 exact、time-bounded 合同用于 `[2025-07-01,2026-07-01)` 恢复语料，并移除了实现中的单一年份硬编码；`Tournaments.Year` 仅为描述字段，不能替代赛事自身的 `OverviewPage -> League/Region` exact relation。跨年度构建结果与新限制见 `docs/RECOVERY_IDENTITY_SERIES_FEATURES.md`。
