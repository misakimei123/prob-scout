# DATA-007 候选自动匹配

> 验证日期：2026-08-12
> 范围：从 DATA-006 的 `Event`、显式 `TeamAlias` 和 Polymarket `MarketCandidate` 生成匹配状态；不执行 DATA-008 的 50 场人工核验

## 1. 输入与输出

`src/candidate_matching.rs` 接收：

- 一个或多个内部 `Event` 候选；
- Gamma Match Winner 的 event、market、condition、BO、两个有序 outcome/token 和 `Market End`；
- 同一 market 的可选 CLOB `Game Start`；
- 显式 `TeamAlias` 与调用方提供的开赛时间容忍值。

批量入口 `match_market_candidates` 保持 Gamma fixture 的市场顺序。每个结果包含 `Matched`、`NeedsReview` 或 `Rejected`、结构化原因、来源 market ID，以及仅在 `Matched` 时生成的 `MarketMapping`。

## 2. 判定规则

| 证据 | 状态 | 原因 |
|---|---|---|
| 唯一 Event；双方显式 alias、BO 和开赛时间均一致 | `Matched` | 生成正式映射 |
| 缺 alias、alias 歧义、缺 `Scheduled Start` 或缺 CLOB `Game Start` | `NeedsReview` | 证据不足，不能猜测 |
| `Scheduled Start` 与 CLOB `Game Start` 超过容忍值 | `NeedsReview` | 可能是改期或来源延迟，交人工判断 |
| 同双方、同 BO 对应多个 Event | `NeedsReview` | 无法唯一选择 |
| 双方身份、BO 或 Event 来源队伍证据直接矛盾 | `Rejected` | 已有证据不一致 |
| 必填 market 字段无效、两个 outcome 解析为同一队伍、正式合同校验失败 | `Rejected` | 候选不满足最小合同 |

只有显式 alias 参与解析。名称规范化仅用于定位已登记 alias，不会根据 `NS`、历史改名、二队名称或字符串相似度创建新身份。

## 3. 时间与 outcome 边界

- 时间差只计算 Leaguepedia 等赛事来源的 `Scheduled Start` 与 CLOB `Game Start`。
- Gamma `Market End` 原样保留，但不参与开赛时间差；真实样例的 6 小时差不会阻止匹配。
- `outcomes[0/1]` 与各自 token 始终按输入 index 构造 `MarketOutcome`，不会按队名或内部 ID 排序。

## 4. 验证样例

定向测试沿用 DATA-006 的真实来源身份：Leaguepedia `LCK/2026 Season/Rounds 3-4_Week 12_1`、Polymarket event/market `816302/3422466`、Leaguepedia `Scheduled Start=08:00Z`、CLOB `Game Start=08:00Z` 和 Gamma `Market End=14:00Z`。

`cargo test --locked --lib candidate_matching` 共 8 个测试，覆盖：

- 唯一候选自动 `Matched`，并保持 outcome/token index；
- 队伍矛盾与 BO 矛盾 `Rejected`；
- 15 分钟开赛时间差在 5 分钟容忍值下进入 `NeedsReview`；
- 缺显式 alias、缺 CLOB `Game Start` 和重复 Event 进入 `NeedsReview`；
- 批量结果保持来源市场顺序。

全库 `cargo test --locked --lib` 共 18 个测试通过。DATA-007 没有创建人工核验表、交易、模型或后续任务代码。
