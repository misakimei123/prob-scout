# DATA-008 50 场映射人工核验

> 核验日期：2026-08-12
> 范围：固定 50 个 recent historical LOL Match Winner 市场，逐场核对 Gamma、CLOB `gst` 与 Leaguepedia；不调查 DATA-009 历史报价等级

## 1. 结论

50 场均已逐行核验。按 DATA-006 已验证的 5 分钟开赛时间容忍值重放 DATA-007：

| 人工期望状态 | 数量 | 人工结论 |
|---|---:|---|
| `Matched` | 29 | 29/29 队伍双方、BO 和开赛时间正确，无自动匹配错误 |
| `NeedsReview` | 21 | 21/21 为同一双方和 BO，但 Leaguepedia `Scheduled Start` 与 CLOB `Game Start` 相差 10–90 分钟，均正确阻止自动匹配 |
| `Rejected` | 0 | 本批真实样本未发现队伍或 BO 硬矛盾 |

因此本批自动 `Matched` 样本错误数为 0。未发现需要修改 DATA-007 规则的 false positive；相反，敏感性检查表明放宽到 30 分钟会把 12 个当前时间冲突静默转为 `Matched`，不符合 fail closed，故保持 5 分钟。

## 2. 输入与可重复性

- Gamma fixture：50 个 recent historical 候选，SHA-256 `a3acf863a220449e5b0e94919e68dcf43025ef877220e9d08d5c944904b65b07`。
- Leaguepedia：固定 `2026-08-08 00:00:00` 至 `2026-08-12 23:59:59` UTC 的 CargoExport 响应，210 rows，SHA-256 `db3d5c906589ace46cac2861b89a510e23e83b49358e07b7e8cbdfd1c9fc0b78`。
- CLOB：50 个官方 `GET /clob-markets/{condition_id}` metadata 响应；脚本逐个校验 `gst` 与 Gamma outcome/token index。
- 核验表：[DATA_008_MAPPING_REVIEW.csv](./DATA_008_MAPPING_REVIEW.csv)，50 rows，SHA-256 `7fa2aa3d5ce52cf7f61041a2c94ef268120ee2828c3027f26531c2e1738d5d27`。

`research/prepare_mapping_review.ps1` 缓存上述 raw evidence 和 hash manifest。`-Offline` 完整重放读取 210 条 Leaguepedia rows 与 50 个 CLOB cache，网络请求为 0；5 分钟自动草表 hash 为 `76625ed6680be8992b2e8f0b02dbba38292173d654db4d7e35c0ed055b3094f9`。

## 3. 人工核验方法

每一行独立检查：

1. Gamma title 和两个 outcome 是否确为 LOL BO1/BO3/BO5 Match Winner；
2. CLOB token `t` / outcome `o` 是否逐 index 等于 Gamma `clob_token_ids[i]` / `outcomes[i]`；
3. Leaguepedia `MatchId`、双方和 BO 是否指向同一系列赛；
4. 显示名不同的 11 场只在明确赛事、双方、BO 和时间证据一致时记录显式 alias，不用 fuzzy similarity 猜测；
5. Leaguepedia `Scheduled Start` 与 CLOB `Game Start` 的绝对差是否超过 300 秒；Gamma `Market End` 只保留，不参与该判断。

核验表保留 source ID、双方原名、两个 token、三种来源时间、BO、时间差、人工结果和错误分类，能够逐行复核。

## 4. 错误分类

| 分类 | 数量 | 处理 |
|---|---:|---|
| `none` | 29 | 自动 `Matched`，人工确认正确 |
| `start_time_conflict` | 21 | `NeedsReview`，不生成正式映射 |
| `team_pair_mismatch` | 0 | 未观察到 |
| `best_of_mismatch` | 0 | 未观察到 |
| `outcome_token_order_mismatch` | 0 | 未观察到；50/50 CLOB index 校验通过 |

21 个时间冲突的差值分布为：600 秒 3 场、900 秒 3 场、1200 秒 2 场、1500 秒 1 场、1800 秒 3 场、2700 秒 2 场、3300 秒 1 场、3600 秒 4 场、5100 秒 1 场、5400 秒 1 场。

## 5. 自动验收

Rust 测试 `replays_all_fifty_manually_reviewed_mappings_without_false_matches` 读取完整 CSV，逐行构造 DATA-006 `Event` / `TeamAlias` / `MarketCandidate` 并调用 DATA-007 matcher。测试断言：

- 恰好 50 个唯一 market ID；
- 29 个 `Matched` 均生成 outcome/token 保序的 `MarketMapping`；
- 21 个 `NeedsReview` 均以 `StartTimeConflict` 拒绝生成 mapping；
- Gamma `Market End` 不改变匹配状态。

本任务只验证映射准确性。没有抓取 order book depth、bid/ask 或 price history，也没有对历史市场作 Grade A/B/C 判定；这些严格留给 DATA-009。
