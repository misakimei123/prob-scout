# M3R-003 新时间化身份、赛果与特征

更新日期：2026-08-13

## 1. 任务边界

本任务只消费 M3R-002 的 3,759 条 source-identity candidates，重建以下三层数据：

1. exact、time-bounded Team/Competition identity evidence 与 coverage audit；
2. 只包含 identity fully resolved candidates 的 Series Result；
3. 固定 `T-15m` cutoff 的 Feature Snapshot。

本任务不建立 development/final split，不读取或复用已退役的 356 场 Final Test，不训练模型，也不启动 `BACK-001`。

## 2. Identity 合同

M3R-003 复用 HIST-010 的 fail-closed 语义，但移除了仅适用于 2025H1 的年份硬编码：

- Team 只接受 `TeamRedirects.AllName -> canonical page` exact relation；
- Competition 只接受赛事自身 `OverviewPage -> League/Region` exact relation；
- exact relation 与 candidate 的 Scheduled Start 共同形成 `[start,start+1s)` evidence period；
- `Tournaments.Year` 仅是描述字段，不是 identity key，也不能替代 `OverviewPage` relation；
- Missing/Ambiguous 不使用 slug、fuzzy、字符串包含或 source-key fallback。

这一调整允许同一 pipeline 审核跨 2025/2026 的恢复语料，但没有放宽 Canonical identity 的证据门槛。

## 3. 真实构建结果

### Identity coverage

- candidates：3,759；
- unique team source keys：694，其中 Resolved 492、Missing 202、Ambiguous 0；
- unique competition source keys：267，全部 Resolved；
- fully resolved candidates：3,155（83.93%）；
- blocked candidates：604（16.07%）；
- team occurrences：Resolved 6,661、Missing 857、Ambiguous 0；
- competition occurrences：Resolved 3,759、Missing 0、Ambiguous 0；
- review queue：202 条，全部为缺失 team exact relation。

Identity audit SHA-256：`8f3e7aeadc9cf071adbe21fd74becd52126cd720fbe017b45b4755964d7bb331`。

### Series Result

- eligible rows：3,155；
- 时间分布：2025 年 1,281、2026 年 1,874；
- 赛制分布：BO3 2,335、BO5 820；
- 覆盖 25 个 Patch、9 个 Region；
- 与旧 M2 1,778 场 corpus 的 `series_id` overlap：0。

Series Result SHA-256：`dff9c9ee61cabf0c3a5a0a6aa9518fcd02cf6d28aa02a1cae6d6cd6a7817e6ac`。

### T-15m Feature Snapshot

- snapshots：3,155，与 Series Result 成员集合完全一致；
- 至少一侧存在可用历史：3,130；
- source-time violations：0；
- snapshot lead violations：0；
- 目标赛后字段泄漏：0；
- 6,310 个 team sides 中 same-Patch unavailable 2,429（38.49%）；
- 6,310 个 team sides 中 prior-series unavailable 166（2.63%）；
- 历史查询得到 17,233 条 unique game rows；1,798 个未完成历史 series 按 fail-closed 排除。

Feature Snapshot 首次构建和 cache replay 的 SHA-256 均为 `8433cc10ee73cab042049d0afe0f81cfc0d96504348346178fb6c4baaa3c7f2b`。

## 4. Lineage 与复现

版本化输出：

- `data/processed/lol-historical-identity-evidence/2026-08-13.f42324d.m3r003-identity-v1/`
- `data/processed/lol-series-results/2026-08-13.f42324d.m3r003-series-v1/`
- `data/processed/lol-prematch-features/2026-08-13.f42324d.m3r003-features-v1/`
- `data/processed/lol-prematch-features/2026-08-13.f42324d.m3r003-features-replay-v1/`

Identity manifest 固定 1 个 M3R-002 upstream 和 33 个 raw Cargo pages；Series manifest 固定 Identity upstream；Feature manifest 固定 Series upstream 和 67 个 raw Cargo pages。首次 Feature 构建与 cache replay 使用不同 version，但输出 hash 相同。对 100 个 raw inputs、全部 upstream outputs 和三层 processed outputs 的 SHA-256 复核均通过。

manifest 证明本次构建实际使用的字节及其 lineage，不证明 Leaguepedia 页面在赛事发生后从未被修改。

## 5. 验证

- `cargo test --locked --lib`：88/88 passed；
- `cargo fmt --check`：passed；
- `cargo check --locked`：passed；
- 两个变更 PowerShell 脚本的 parser：2/2 passed；
- Identity、Series、Feature、Feature replay Dataset Manifest Rust 校验：4/4 passed；
- 108 个唯一 lineage 文件（100 raw pages、processed outputs 与 upstream manifests/outputs）SHA-256 复核：passed；
- `git diff --check`：passed。

## 6. 验收结论与限制

M3R-003 已满足任务验收：identity 继续 exact、time-bounded，Missing/Ambiguous fail closed，Series/Feature 成员一致，`T-15m` source-time leakage 为 0，且离线 replay 确定性成立。

反方证据和剩余限制必须继续保留：

- 202 个 team source keys 没有 exact relation，导致 604 candidates 不得进入下游；
- same-Patch unavailable 仍为 38.49%，不能解释为 0% form；
- 本任务没有建立新的 Development 或 sealed Final Test；
- 本任务没有产生模型有效性、策略收益、可成交性或 execution readiness 证据。

下一任务仅为 `M3R-004`：基于这 3,155 条新成员建立连续、唯一、无重叠的新 Development 与从未公开成员的 sealed Final Test。
