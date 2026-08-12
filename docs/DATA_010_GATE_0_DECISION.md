# DATA-010 Gate 0 决策

决策日期：2026-08-12

## 结论：Conditional Go

允许进入 M2/M3 的 **Grade C 信号研究**，但不允许把历史结果表述为可成交回测，也不把实时订单簿持续采集、WebSocket 监听或真实执行视为已验证。结论置信度为中等：50 场映射与历史价格证据可复现，但实时盘口稳定性只有单市场 REST 快照，尚无持续采集证据。

## 核心证据

| Gate 维度 | 实际证据 | 判断 |
|---|---|---|
| 映射质量 | [50 场人工核验](./DATA_008_MAPPING_REVIEW.md)包含 29 个 `Matched`、21 个 `NeedsReview`、0 个 `Rejected`；29/29 自动 `Matched` 无人工发现的错误，21/21 时间冲突均正确 fail closed | 通过抽检准确性；只有 29 场可直接进入下游，21 场不得静默放行 |
| 关键字段 | 50 场的 market/event/condition、双方 token、BO、CLOB/Leaguepedia 时间、赛事 ID、双方队伍和人工结论共 600/600 个必查单元非空 | 当前固定样本为 100%；不外推到未来批次 |
| 历史市场数据 | [DATA-009](./DATA_009_HISTORICAL_MARKET_COVERAGE.md)为 A `0/50`、B `0/50`、C `50/50`、Unavailable `0/50`；100/100 token 可离线重放 | 只满足信号研究；不能计算历史 spread、depth、10U VWAP、slippage、fill failure 或当时 fee |
| 实时 Market Quote | [DATA-005](./POLYMARKET_ORDER_BOOK_SAMPLE.md)对 1 个开放市场保存双方完整 depth、bid/ask、fee、book/local timestamp，并可离线重放含费 10U 理论 fill | 已证明公开 CLOB order book 可读取，但单次 REST 样本不能证明持续稳定性 |
| 数据用途 | [Source Registry](./DATA_SOURCES.md)允许 Polymarket read-only 与 Oracle's Elixir/Leaguepedia 条件性本地 Research/Paper；GRID 阻塞，Riot Developer API 排除 | 允许当前本地研究；不授权真钱、商业化或 raw 再分发 |

## 推理与反证

直接 `Go` 不成立：42%（21/50）候选因 10–90 分钟开赛时间冲突仍需人工复核；历史报价 100% 为 Grade C；实时盘口只有 1 个 REST snapshot，没有 WebSocket、重复轮询、断线重连、重新同步、乱序或重复事件验证。因此“实时订单簿已经稳定采集”和“历史 PnL 可成交”都缺少证据。

`Kill` 也不成立：目标 LOL Match Winner 市场、condition/token ID 和双方 outcome 可以识别；29 个自动映射没有观察到 false positive；全部 50 场都有双方 T-15m 前 price history；公开 CLOB 已实际返回可计算 10U 理论 fill 的完整订单簿。现有缺口表示需要降级与继续验证，而不是数据路线已经不可行。

## 继续条件与禁止事项

1. M2/M3 只做结果数据集、无未来泄漏特征和概率模型；自动输入仅接受 `Matched`，`NeedsReview` 必须人工解决或排除。
2. Market Baseline 可使用同一信息时点的 Grade C `p`，但必须与可成交 ask/depth 分开命名和报告；历史 ROI/PnL 不作为通过证据。
3. 在任何 execution-sensitive 结论前，必须跨多个未来市场持续保存不可变 order book、fee、book timestamp 和本地接收时间，并验证离线重放。
4. WebSocket 若作为实时采集路径，必须明确验证订阅恢复、断线重连、全量 book 重新同步、乱序和重复事件；当前尚未实现。REST polling 若保留为 fallback，也必须有 freshness 和连续失败门禁。
5. HIST-003 必须核对赛事胜者与市场 resolution；若无法可靠对齐，不得生成训练 label。
6. GRID、Riot Developer API、认证 CLOB、钱包和真实下单继续保持阻塞；来源许可或条款变化时重新审核。

当目标市场无法稳定识别、结果与 resolution 无法对齐、研究用途不再允许，或持续实时盘口采集验证失败时，应重新进入 Gate 0 并考虑 `Kill`。本次决定不启动 HIST-001，也不实现 WebSocket 或交易代码。
