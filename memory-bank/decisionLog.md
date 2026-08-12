# Decision Log

## 2026-08-12 — Research First 与 Prematch Match Winner

第一版只验证职业 LOL 盘前系列赛胜负市场。原因是先隔离概率模型和市场错价能力，避免盘中状态、退出执行和额外市场类型同时引入混杂变量。影响是盘中交易、一血、一龙、一塔和单局市场均不属于当前版本。

## 2026-08-12 — 双策略共享输入、独立账本

Threshold Strategy 与 Edge Strategy 读取同一 Prediction 和同一 Market Quote，但各自维护账本和风险暴露。原因是直接比较高概率买入与净 Edge 过滤的效果，不能让资金共享污染实验结论。

## 2026-08-12 — 市场价格必须使用可成交成本

策略使用 order book depth、VWAP 和当时 fee 计算 `effective_entry_price`，不能使用页面展示概率代替。原因是 midpoint、ask、spread、深度和 fee 会改变真实期望值。

## 2026-08-12 — 来源时间不折叠

Leaguepedia `Scheduled Start`、Gamma `Market End` 与 CLOB `Game Start` 分别保存。原因是真实样例中 Gamma endDate 比 CLOB gst 晚 6 小时；静默合并会破坏盘前门禁。影响是自动匹配只比较具有开赛语义的时间，冲突必须进入人工检查或拒绝。
