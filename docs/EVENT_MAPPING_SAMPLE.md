# DATA-006 Event 与 Market Mapping 合同

> 验证日期：2026-08-12
> 范围：定义最小 `Event`、`TeamAlias`、`MarketMapping`，不实现 DATA-007 自动匹配

## 1. 结论

统一映射合同已建立，并使用同一场真实赛事的 Leaguepedia、Polymarket Gamma 与 CLOB 字段验证。合同能够解释来源 ID、双方原始队名、outcome/token 顺序和各自时间语义；不会把 Gamma `endDate` 静默解释为开赛时间。

## 2. 最小结构

- `Event`：内部系列赛 ID、游戏、赛事、BO 类型、双方内部队伍 ID、来源证据。
- `EventSourceEvidence`：来源赛事 ID、来源双方队名、来源时间及其语义。
- `TeamAlias`：来源队伍 ID/名称到内部队伍 ID 的显式映射。
- `MarketMapping`：Polymarket event/market/condition ID、两个有序 outcome/token、Gamma market end 与 CLOB game start。

当前名称规范化只执行 Unicode lowercase，并删除空白和标点。它可以把 `Nongshim RedForce` 与 `Nongshim Red Force` 生成相同规范名，但不会猜测 `NS`、历史改名或二队关系；这些情况必须通过显式 `TeamAlias` 解决。

## 3. 可追溯样例

| 字段 | 值 |
|---|---|
| Internal Event | `lol:lck:2026-08-12:dns-ns` |
| Leaguepedia MatchId | `LCK/2026 Season/Rounds 3-4_Week 12_1` |
| Leaguepedia teams | `DN SOOPers` / `Nongshim RedForce` |
| Leaguepedia Scheduled Start | `2026-08-12T08:00:00Z` |
| Polymarket Event / Market | `816302` / `3422466` |
| Condition ID | `0x621f09a374447eb0965f70f78e67bb79dd773e7ca76a7646f1dd94b787597968` |
| Gamma outcome names | `DN SOOPers` / `Nongshim Red Force` |
| Gamma Market End | `2026-08-12T14:00:00Z` |
| CLOB Game Start | `2026-08-12T08:00:00Z` |
| Scheduled/CLOB start span | `0` 秒 |
| Gamma end - CLOB start | `21,600` 秒（6小时） |

两个 token 按 Gamma outcome index `0/1` 原序保存，禁止按规范队名或字母顺序重排。

Leaguepedia 的本场字段通过其公开 CargoExport 查询取得；Gamma/CLOB 字段来自 DATA-004/DATA-005 已缓存并校验 hash 的本地 fixture。仓库不提交新增 raw 响应。

## 4. 失败边界

- 缺少任一来源队名对应的显式 alias：合同校验失败。
- outcome index 不是严格的 `0/1`：合同校验失败。
- outcome 对应的内部队伍与 Event 双方不一致：合同校验失败。
- `best_of` 不是 `1/3/5`：合同校验失败。
- Gamma Market End 与比赛开始不同：分别保存，不直接判为赛事冲突。
- Leaguepedia Scheduled Start 与 CLOB Game Start 超过调用方容忍值：`has_start_time_conflict` 返回 true，后续 DATA-007 必须进入 `NeedsReview` 或 `Rejected`。

## 5. 当前边界

DATA-006 只定义合同、校验和解释能力。它没有批量读取 fixture、自动生成内部 Event ID、推断别名、决定匹配状态或写入真实样本数据库；这些属于 DATA-007。
