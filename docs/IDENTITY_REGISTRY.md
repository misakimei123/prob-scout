# HIST-002 队伍与赛事身份合同

审核日期：2026-08-12

范围：基于 DATA-008 固定 50 场人工核验建立可复核的队伍名称变体和赛事品牌映射；不生成 HIST-003 series result，不推断未观察时段的身份，也不把队伍关系或市场 resolution 当作已验证。

## 1. 结论

- 50/50 场的双方身份和赛事品牌均有人工核验记录；每条新映射通过 `evidence_review_ids` 回到 [`DATA_008_MAPPING_REVIEW.csv`](./DATA_008_MAPPING_REVIEW.csv) 的准确观察时间、市场和来源名称。
- 12 组无法仅靠现有基础规范化得到的跨来源队名已写入 [`HIST_002_TEAM_ALIAS_REVIEW.csv`](./HIST_002_TEAM_ALIAS_REVIEW.csv)，全部为 `verified_explicit`。
- 21 组 Gamma stage label 与 Leaguepedia season/overview ID 已写入 [`HIST_002_COMPETITION_MAPPING.csv`](./HIST_002_COMPETITION_MAPPING.csv)，汇总为 17 个 `Canonical Competition`。
- 其余队名在 DATA-008 中规范化后相同，但仍以对应 review row 作为显式来源证据，不把字符串相等当作全历史身份保证。
- 本批证据没有提供可审核的正式改名生效区间，因此 **verified rename periods 为 0**。没有为了满足“改名表”而虚构改名；未来只能在取得旧名、新名、有效时间和 evidence ref 后新增 `Team Identity Period`。

## 2. 领域边界

Rust 合同位于 [`src/identity_registry.rs`](../src/identity_registry.rs)：

- `CanonicalTeam` 是跨来源稳定身份；Academy、Challengers、二队默认是独立队伍，不能因共享母队字符串合并。
- `TeamIdentityPeriod` 以半开区间 `[valid_from_utc, valid_until_utc)` 表示某个来源 ID/名称的有效身份。缺少、过期或多个候选均 fail closed。
- `CanonicalCompetition` 是联赛或杯赛品牌；season、split、group、playoff 是来源观察名或阶段，不是单场 `Event`。
- `CompetitionIdentityPeriod` 记录来源赛事 ID/名称在某个时间区间内指向哪个品牌。
- source ID 存在时只按 source ID 解析；未知 ID 不回退到名称。只有来源没有稳定 ID 时，才按已登记的 source + normalized name + observation time 解析。
- 名称相似度、字符串包含、缩写猜测、母队关系和当前网页名称都不是身份依据。

## 3. 持久化

[`202608120003_create_identity_registry.sql`](../migrations/202608120003_create_identity_registry.sql) 新增：

- `canonical_teams`
- `team_identity_periods`
- `canonical_competitions`
- `competition_identity_periods`

identity period 保存 source ID、原名、规范名、有效区间和 `evidence_ref`。数据库允许真实歧义作为多条证据存在，由解析器返回 `Ambiguous`；不能用 UNIQUE 约束静默选定某个 identity。无效区间、空 source ID 和未知 canonical foreign key 会被拒绝。

## 4. 反例校验

| 场景 | 结果 |
|---|---|
| `LOS` 与 `LØS` 在 review 15/47 的观察时点 | 显式解析到 `lol-team:los` |
| 旧名和新名具有不重叠的已审核区间 | 可解析到同一 Canonical Team |
| source ID 未登记，但展示名碰巧相同 | `Missing`，不回退名称 |
| 同一 source/name/time 指向两个队伍 | `Ambiguous`，调用方不得选第一个 |
| `LCK Round 3-4 Rise Group` 与 Leaguepedia season ID | 解析到 `lol-competition:lck`，不创建新 Event |
| `DN SOOPers` 与 `DN SOOPers Challengers` | 保持不同 Canonical Team |

## 5. 对后续任务的约束

- HIST-003 只能消费 `Resolved` identity；`Missing` 和 `Ambiguous` 记录进入人工队列或排除。
- HIST-003 必须另外核对 series winner 和 market resolution；身份解析成功不等于结果 label 正确。
- processed identity dataset 必须遵守 [`DATASET_LAYOUT.md`](./DATASET_LAYOUT.md) 的 Manifest v1，记录 raw hash、生成时间和代码版本。
- 本合同没有改变 Gate 0 的 Grade C 限制，也没有实现 WebSocket、策略或交易代码。
