# M3R-002 非重叠恢复候选语料

更新日期：2026-08-13

范围：只构建时间晚于旧 Final Test 的 Leaguepedia source-identity candidate corpus，并证明它与旧 1,778 条 eligible corpus 在成员和时间上均无重叠。本任务不解析 Canonical Team/Competition，不生成 Series Result、Feature Snapshot、split 或模型。

## 1. 恢复边界

新语料固定使用半开窗口 `[2025-07-01T00:00:00Z, 2026-07-01T00:00:00Z)`。旧 corpus 通过以下 immutable upstream 固定：

```text
data/processed/lol-series-results/2026-08-13.8db1666.hist010-series/
└── series-results.csv.manifest.json
```

生成器只从旧 CSV 读取 `series_id` 与 `scheduled_start_utc`，并执行以下 fail-closed 校验：

- reference manifest、output hash 与 row count 必须一致；
- 旧 corpus 的每条时间必须严格早于恢复边界；
- 新 candidate 的每条时间必须不早于恢复边界；
- 新旧 `series_id` 不得相同；
- `max(old scheduled_start_utc) < min(new scheduled_start_utc)` 必须成立。

任何一项失败都不生成有效 manifest。输出 audit 中的 `recovery_disjointness` 固定 reference/new 时间范围与两类 overlap count。

## 2. Region coverage 的语义

M3R-002 额外消费完整分页的 Leaguepedia `Tournaments` raw，以 `OverviewPage -> Region` exact relation统计候选覆盖。该字段只作为 source coverage：

- 不生成 Canonical Competition ID；
- 不规范化 source Region 文本，因此 `Americas` 与 `North America` 保持不同 source value；
- 缺失 relation 计入 `missing_candidate_count`；
- 同一 `OverviewPage` 对应多个 Region 或空值冲突计入 `ambiguous_candidate_count`；
- Team/Competition 的时间化 identity resolution 仍只允许在 M3R-003 执行。

## 3. 真实构建

```powershell
./research/build_historical_candidate_corpus.ps1 `
  -StartUtc "2025-07-01 00:00:00" `
  -EndUtc "2026-07-01 00:00:00" `
  -Version "2026-08-13.8db1666.m3r002-h2-2025-v2" `
  -ReferenceSeriesManifest "data/processed/lol-series-results/2026-08-13.8db1666.hist010-series/series-results.csv.manifest.json" `
  -MinimumRecoveryStartUtc "2025-07-01 00:00:00" `
  -MinimumCandidateCount 700 `
  -MinimumDistinctUtcDates 250 `
  -MinimumDistinctPatches 10 `
  -MinimumDistinctRegions 5 `
  -MinimumBo3Count 500 `
  -MinimumBo5Count 100
```

| 指标 | 数值 |
|---|---:|
| MatchSchedule pages / rows | 34 / 16,598 |
| ScoreboardGames pages / rows | 48 / 23,582 |
| Tournaments pages | 21 |
| total raw pages | 103 |
| candidates / rejections | 3,759 / 12,839 |
| distinct UTC dates | 349 |
| distinct Patch source keys | 25 |
| BO3 / BO5 | 2,819 / 940 |
| distinct Region source values | 9 |
| Region resolved / missing / ambiguous candidates | 3,759 / 0 / 0 |

Region source coverage 为 Americas 190、Asia 1、Asia Pacific 298、Brazil 156、China 308、EMEA 1,853、International 158、Korea 627、North America 168。

主要 rejection 为 `unsupported_best_of` 8,930、`missing_patch` 3,726、`invalid_score` 98、`duplicate_teams` 31、`game_count_mismatch` 24、`completion_not_after_start` 20、`missing_required_field` 6、`winner_mismatch` 4。candidate 与 rejection 合计严格等于 16,598 个 distinct MatchId。

## 4. 零重叠与 lineage 证据

| 证明项 | 结果 |
|---|---|
| old reference rows | 1,778 |
| old Scheduled Start | `2025-01-12T09:00:00Z` – `2025-06-30T18:30:00Z` |
| new candidate Scheduled Start | `2025-07-01T16:00:00Z` – `2026-06-30T08:00:00Z` |
| member overlap | 0 |
| temporal overlap | 0 |

processed artifact：

```text
data/processed/lol-historical-series-candidates/
└── 2026-08-13.8db1666.m3r002-h2-2025-v2/
    ├── historical-candidate-audit.json
    └── historical-candidate-audit.json.manifest.json
```

- dataset SHA-256：`f5c4210a04417392c92801a8d5f9e7d6c2b7c9f2871e63bd6e89d77f3d32860b`
- manifest SHA-256：`ffeb27fd341a5dbe678ec7945e694e2a2fd98d16658f8fb9f4bc862df444962a`
- manifest 固定 103 个 raw page、1 个旧 corpus upstream、生成代码/参数、dirty diff hash、output hash/row count/time range。

相同输入双重 Rust 构建 hash 一致；Rust Dataset Manifest v1 校验通过；独立 PowerShell 复核全部 103 个 raw hash、upstream manifest/output hash、output hash，以及 1,778 对 3,759 的成员/时间零重叠。

## 5. 下一步边界

M3R-002 只证明新 source candidate 的规模、结构、来源覆盖与独立性，不代表 3,759 条均为 eligible Series Result。下一任务 M3R-003 必须重新执行 exact、time-bounded Team/Competition identity resolution，`Missing`/`Ambiguous` fail closed，再生成 Series Result 与 `T-15m` Feature Snapshot。旧 356 场 Final Test 继续只作为 retired diagnostic evidence，不能进入新模型选择或新 Final。
