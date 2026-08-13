# HIST-009 时间化 Identity Coverage Audit

更新日期：2026-08-13

本合同对 HIST-008 的 source-identity candidates 执行时间化 Canonical identity coverage 审计。它只消费 HIST-002 已有的显式证据，不执行 fuzzy、slug、包含关系或当前名称回填，也不生成 Series Result、Feature Snapshot 或模型输入。

## 1. 输入与输出

输入：

- HIST-008 `historical-candidate-audit.json` 及其 Dataset Manifest v1；
- `DATA_008_MAPPING_REVIEW.csv` 中每条 evidence 的 CLOB Game Start 观测时刻；
- `HIST_002_TEAM_ALIAS_REVIEW.csv` 的显式 team alias；
- `HIST_002_COMPETITION_MAPPING.csv` 的显式 competition mapping。

构建器将三份 review CSV 复制为 hash 命名的不可变 `data/raw/identity_coverage/hist002/` 快照，并将 HIST-008 记录为 upstream processed dataset。输出位于：

```text
data/processed/lol-identity-coverage-audits/<version>/
├── identity-coverage-audit.json
└── identity-coverage-audit.json.manifest.json
```

可重复构建命令：

```powershell
./research/build_identity_coverage_audit.ps1 `
  -CandidateManifest data/processed/lol-historical-series-candidates/<version>/historical-candidate-audit.json.manifest.json `
  -Version <new-immutable-version>
```

已存在的 processed version 不允许覆盖。脚本先校验 upstream manifest/output hash，再用相同输入执行两次 Rust 构建并比较输出 SHA-256，最后校验数量守恒和新 manifest。

## 2. 时间化解析合同

每条 candidate 在其 `scheduled_start_utc` 调用既有 `IdentityRegistry`：

- source 固定为 `Leaguepedia`；
- source ID 缺失时，只允许 source、显式登记名称和 observation time 同时命中；
- `Resolved` 必须恰好命中一个 active Canonical identity；
- 无 active period 为 `Missing`；
- 同时命中多个 Canonical identity 为 `Ambiguous`，不得选择第一个；
- team 双方和 competition 全部 `Resolved` 时，series 才是 `fully_resolved`。

HIST-002 的现有 evidence 来自 2026-08 DATA-008 review。它只证明对应观测秒内的身份关系，不能倒推为覆盖 2025H1。构建器为每条 evidence 保留半开区间 `[observed_at, observed_at + 1s)`，防止把当前映射无限回填到历史。

## 3. 输出结构

`series_resolutions[]` 为每条 candidate 保存 source key、解析状态、resolved Canonical ID 或全部 ambiguous candidate IDs，以及 `fully_resolved` 门禁结果。

`review_queue[]` 只保存未解析项，并按 `identity_kind + source_key + status + ambiguous IDs` 聚合。每项保留首次/末次出现时间、occurrence count 和全部 affected series IDs，从而避免按比赛重复人工补证，同时不丢失时间范围和影响面。

## 4. 2025H1 真实审计

固定版本：`2026-08-13.8db1666.hist009`

| 指标 | 结果 |
|---|---:|
| HIST-008 candidates | 2,061 |
| Fully resolved series | 0 |
| Blocked series | 2,061 |
| Team occurrences: Resolved / Missing / Ambiguous | 0 / 4,122 / 0 |
| Competition occurrences: Resolved / Missing / Ambiguous | 0 / 2,061 / 0 |
| Distinct team source keys | 468 |
| Distinct competition source keys | 146 |
| Aggregated review queue | 614 |
| Event time range | 2025-01-12 09:00Z – 2025-06-30 18:30Z |

Review queue 由 468 条 team `Missing` 和 146 条 competition `Missing` 构成。结果不是“没有候选”，而是现有 2026 显式 evidence 对 2025 观测时刻没有 active period。它量化了历史 identity evidence 的真实缺口。

输出 SHA-256：`a868952c4e6e1b0872d5786faa338d5c52dcefed724b15a9969252e263529b82`

Manifest SHA-256：`4b24eabdfc9a8dafc46a773cfc0dfb8599aef3495147dcefb6f5cfa0f2b1784b`

## 5. Gate 结论

HIST-009 完成时 M2 数据就绪 Gate 仍为 `NotReadyForM3`，0 条 fully resolved candidate 不能进入扩展重建。该历史状态已由 HIST-010 exact Cargo evidence supersede；当前结果为 1,778 fully resolved 与 `ReadyForM3`，详见 `HISTORICAL_IDENTITY_EVIDENCE.md`。
