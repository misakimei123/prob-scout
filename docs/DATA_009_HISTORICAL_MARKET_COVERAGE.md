# DATA-009 历史市场数据等级覆盖报告

审核日期：2026-08-12

固定样本：[`DATA_008_MAPPING_REVIEW.csv`](./DATA_008_MAPPING_REVIEW.csv) 的 50 个 recent historical LOL Match Winner markets

逐场证据：[`DATA_009_HISTORICAL_MARKET_GRADES.csv`](./DATA_009_HISTORICAL_MARKET_GRADES.csv)

复跑脚本：[`../research/audit_historical_market_data.ps1`](../research/audit_historical_market_data.ps1)

## 1. 结论

本次固定 50 场样本全部只能判为 **Grade C**：

| 覆盖等级 | 全部 50 场 | 自动 `Matched` 29 场 | `NeedsReview` 21 场 |
|---|---:|---:|---:|
| Grade A：决策时点完整或足够 depth，且有当时 fee 证据 | 0（0%） | 0 | 0 |
| Grade B：决策时点 best bid/ask，但没有完整 depth | 0（0%） | 0 | 0 |
| Grade C：只有官方 `{t,p}` price history | 50（100%） | 29 | 21 |
| Unavailable：连双方 price history 都不完整 | 0（0%） | 0 | 0 |

因此，这批历史数据只能用于信号研究。它不能计算历史 10U VWAP、spread、slippage、partial fill、fill failure 或决策时点真实 fee，也**不得称为可成交回测或历史可执行 PnL 证据**。

本报告只完成 DATA-009 的数据等级调查，不作 DATA-010 Gate 0 决策。

## 2. 等级判定合同

决策时点统一取 CLOB `game_start_time - 15 minutes`，每个 outcome token 查询该时点之前 24 小时的官方 price history，`fidelity=1 minute`。任何晚于决策时点的 point 都会触发 fail closed，不进入覆盖结果。

逐场按以下最强可证明证据判级：

1. **Grade A**：有决策时点完整或足够深的 order book snapshot，能够重建 10U fill，并有当时适用的 fee 参数。
2. **Grade B**：有决策时点 best bid 和 best ask，但没有足够 depth；只能做保守的小额近似研究。
3. **Grade C**：只有 midpoint、last trade 或稀疏 price history；只允许研究预测信号。
4. **Unavailable**：双方 token 任一在固定窗口中没有有效 price point；不强行归入 A/B/C。

官方 [`GET /prices-history`](https://docs.polymarket.com/api-reference/markets/get-prices-history) 的响应点只有 `t` 和 `p`，没有 bid、ask、size、depth 或 fee。官方 [`GET /book`](https://docs.polymarket.com/api-reference/market-data/get-order-book) 返回的是请求当下带 timestamp 的 order book snapshot，且没有历史时点参数；[Market WebSocket](https://docs.polymarket.com/market-data/websocket/market-channel) 能实时推送 book、price change 和 best bid/ask，但必须在当时订阅并自行保存。事后调用当前 `/book` 不能补成历史 Grade A/B。

另外，官方说明展示价格通常为 bid/ask midpoint，spread 大于 0.10 时改用 last trade；买入实际成交在 ask，卖出实际成交在 bid。由此也不能把单一展示/历史价格当作成交价。参见 [Prices & Orderbook](https://docs.polymarket.com/concepts/prices-orderbook)。

## 3. 实际字段覆盖

| 决策时点证据 | 覆盖场数 | 可支持的计算 |
|---|---:|---|
| 完整/足够 depth snapshot | 0/50 | 无法重建 10U 多档成交与 fill failure |
| 同时存在 best bid 和 best ask | 0/50 | 无法计算历史 spread 或保守 crossing price |
| 当时适用的 fee 参数 | 0/50 | 无法证明历史净成交成本 |
| 双方 outcome token 均有 `{t,p}` history | 50/50 | 只支持 Grade C 信号研究 |

100 个 outcome token 均返回决策时点以前的有效 point：

- 每个 token 的 point 数最少 2、最多 1,440，中位数 1,440；`market_id=3482350` 的双方各只有 2 个 point，是明确的稀疏历史样本。
- 最后一个 point 距决策时点最多 52 秒；100 个 token 的中位 staleness 为 49 秒。
- “point 接近决策时点”只说明时间覆盖，不改变字段等级；没有 bid/ask、size 和 depth 时仍为 Grade C。

## 4. 可复现性与失败边界

在线运行：

```powershell
./research/audit_historical_market_data.ps1
```

离线重放：

```powershell
./research/audit_historical_market_data.ps1 -Offline
```

在线首次运行保存 100 份官方原始 JSON 和 100 份含 request URL、采集时间、local path、SHA-256 的 manifest 到 `data/raw/historical_market_grade/`；这些 raw 数据受 `.gitignore` 保护。离线重放命中 100/100 cache，并生成相同的覆盖 CSV：

```text
SHA-256  3a35d45259d057a485c7ddc668bf0411bb199a5cded2259f49fa87c2c4800414
```

脚本还执行以下 fail-closed 检查：固定样本必须恰好 50 场、market ID 不重复、双方 token ID 必须存在、cache hash 必须匹配、响应必须含 `history`、point 不得越过查询窗口、价格必须在 `[0,1]`。任何一项失败都会停止审计，不以缺失或事后数据补齐。

## 5. 残余限制

- 覆盖结论只针对 DATA-008 固定的 50 场 recent historical 样本，不外推为 Polymarket 所有历史市场的永久能力。
- “官方当前文档未提供历史 order book/bid-ask 查询”不等于历史上从未存在第三方归档；若未来引入第三方数据，必须单独审核来源、时间戳、字段语义、许可和完整性，不能直接沿用本等级。
- `NeedsReview` 的 21 场仍有 DATA-008 已记录的开赛时间冲突；它们保留在数据覆盖分母中，但不得在后续建模前绕过映射审核状态。
- 要获得 Grade A，后续只能在赛前实时订阅或轮询并持久化不可变的 order book、quote received time 与 fee 配置；这是后续任务范围，本轮不实现。
