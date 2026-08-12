# Decision Log

## 2026-08-12 — Research First 与 Prematch Match Winner

第一版只验证职业 LOL 盘前系列赛胜负市场。原因是先隔离概率模型和市场错价能力，避免盘中状态、退出执行和额外市场类型同时引入混杂变量。影响是盘中交易、一血、一龙、一塔和单局市场均不属于当前版本。

## 2026-08-12 — 双策略共享输入、独立账本

Threshold Strategy 与 Edge Strategy 读取同一 Prediction 和同一 Market Quote，但各自维护账本和风险暴露。原因是直接比较高概率买入与净 Edge 过滤的效果，不能让资金共享污染实验结论。

## 2026-08-12 — 市场价格必须使用可成交成本

策略使用 order book depth、VWAP 和当时 fee 计算 `effective_entry_price`，不能使用页面展示概率代替。原因是 midpoint、ask、spread、深度和 fee 会改变真实期望值。

## 2026-08-12 — 来源时间不折叠

Leaguepedia `Scheduled Start`、Gamma `Market End` 与 CLOB `Game Start` 分别保存。原因是真实样例中 Gamma endDate 比 CLOB gst 晚 6 小时；静默合并会破坏盘前门禁。影响是自动匹配只比较具有开赛语义的时间，冲突必须进入人工检查或拒绝。

## 2026-08-12 — Gate 0 Conditional Go

M1 只以 `Conditional Go` 进入后续 Grade C 信号研究。原因是 50 场映射中 29 个自动 `Matched` 无观察错误、全部市场有双方历史 `{t,p}`，且单个开放市场的完整 REST order book 可读取和离线重放；反面证据是 21 场仍需人工复核、历史数据全部为 Grade C，且尚无 WebSocket 或持续实时采集稳定性验证。影响是 M2/M3 可继续，但 unresolved mapping 必须排除或解决，历史结果不得声称可成交 PnL，任何 execution-sensitive 结论前必须补足多市场持续 order book、fee、时间戳、重连和离线重放证据。

## 2026-08-12 — 三层数据目录与 Dataset Manifest v1

本地研究数据固定分为不可变 `data/raw/`、可重建 `data/processed/` 和派生 `artifacts/`。每个 processed dataset 必须用 Manifest v1 记录 raw 文件 SHA-256/采集时间、UTC 生成时间、Git commit 与 dirty diff hash、生成入口、输出 hash、row count 和 Event 时间范围。原因是后续队伍身份、结果、特征和数据划分必须能够回到实际输入字节与代码版本；影响是缺失 lineage、路径越界、hash/时间无效或零行的输出都不是有效 dataset。

## 2026-08-12 — 时间化显式身份解析

队伍和赛事品牌身份采用带来源与半开有效区间的显式映射。source ID 存在时只按 ID 解析，未知 ID 不回退名称；没有 ID 时才使用已登记的 source/name/time。原因是改名、名称复用、二队关系和来源修订会让无时间 alias 或 fuzzy match 静默合并不同实体；影响是 `Missing`/`Ambiguous` 必须人工处理或排除，且赛事品牌、赛季阶段与单场 Event 保持分离。

## 2026-08-12 — Series Result 与 Market Resolution 双证据

系列赛 label 只在 Leaguepedia `MatchSchedule` 最终比分/胜者与 Gamma 已关闭、resolved、唯一 0/1 outcome 结算指向同一 Canonical Team 时成立。原因是身份 mapping 只证明“这是同一场/同一队”，不证明结果或市场 outcome label 正确；影响是任一来源未完成、字段缺失或胜者冲突都 fail closed。重复 `series_id` 仅在核心事实完全相同才按证据键字典序合并，否则整组拒绝。
