# Polymarket CLOB 订单簿小样本

> Task：DATA-005
>
> 采集与验证日期：2026-08-12
>
> 用途：ProbScout 本地 Research/Paper；没有认证、钱包或真实订单

## 1. 官方接口与目标市场

- Order book：[公开 CLOB batch books](https://docs.polymarket.com/api-reference/market-data/get-order-books-request-body)
- Market parameters：[CLOB market info](https://docs.polymarket.com/api-reference/markets/get-clob-market-info)
- Fee 规则：[Polymarket Fees](https://docs.polymarket.com/trading/fees)
- Event：`816302`
- Event title：`LoL: DN SOOPers vs Nongshim Red Force (BO3) - LCK Round 3-4 Rise Group`
- Market：`3422466`
- Condition ID：`0x621f09a374447eb0965f70f78e67bb79dd773e7ca76a7646f1dd94b787597968`
- CLOB game start：`2026-08-12T08:00:00Z`
- Quote received：`2026-08-12T06:36:23.4929502Z`
- Request duration：989 ms
- Prematch buffer：15 minutes

该市场在 quote 接收时距离 CLOB `gst` 仍有约 83 分钟，满足盘前采样门禁。

## 2. 关键时间发现

DATA-004 的 Gamma catalog 对同一 event 给出 `endDate=2026-08-12T14:00:00Z`，CLOB market info 给出 `gst=2026-08-12T08:00:00Z`，两者相差 6 小时。

因此确立以下硬约束：

- Gamma `endDate` 只用于目录发现，不得作为是否开赛的唯一判断；
- 盘前下单门禁至少检查 CLOB `gst`；
- DATA-006 映射时还必须把 `gst` 与 Leaguepedia/赛事源时间交叉核对；
- 时间冲突、缺少 `gst` 或距离开赛不足配置 buffer 时 fail closed。

## 3. 可重复命令

在线采集指定开放市场：

```powershell
.\research\capture_polymarket_order_book.ps1 -MarketId '3422466'
```

完全禁止网络并从 raw cache 重新计算：

```powershell
.\research\capture_polymarket_order_book.ps1 `
  -MarketId '3422466' `
  -Offline
```

脚本通过一次 `GET /clob-markets/{condition_id}` 读取 game start、token mapping、minimum/tick 和 fee schedule，再通过一次只读 `POST /books` 同时取得双方完整 order book。`POST /books` 是官方批量 market-data 查询，不是下单接口。

## 4. Raw 与 fixture

| Artifact | Size | SHA-256 |
|---|---:|---|
| CLOB market info raw | 500 bytes | `083f537c982cd96f72ecc160acd0efe83a018cfdf30f1c0eae66935597c3192d` |
| 双 token books raw | 5,956 bytes | `995f12a27fd279c69e457bb1a23ad3084c0ab14537ba99fd12994655f1563ff7` |
| 含费 10U 分析 fixture | 3,827 bytes | `bce21066e484940417fb5a5a1a523b6a52bfec5328243d170b4fffe6aa8692a4` |

离线连续复跑返回 `MarketInfoStatus=cached`、`BooksStatus=cached`、`FixtureStatus=unchanged`。raw、fixture 和 manifest 都保存在 `data/raw/polymarket_clob/`，由 Git 排除。

## 5. Order book 与 10U 理论 fill

`10U` 定义为包含 taker fee 的总 cash cap。脚本不依赖 API 返回顺序，而是按 decimal price 对 bids/asks 重新排序；买入从最低 ask 开始遍历。

| Outcome | Token ID | Bid / Ask | Depth levels B/A | Shares | Book VWAP | Fee | Cash debit | Effective price |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| DN SOOPers | `89601065606835654708323034232613903153677834435591158292528649436426629091306` | `0.38 / 0.39` | 33 / 49 | 24.88212095 | 0.390000 | 0.29597U | 9.99999717U | 0.40189489 |
| Nongshim Red Force | `83918012109539856325069121542829351861121755068443319105428160825083612328645` | `0.61 / 0.62` | 49 / 33 | 15.82828882 | 0.620000 | 0.18646U | 9.99999907U | 0.63178017 |

两个 outcome 的 spread 都是 `0.01`，`tick_size=0.01`，`min_order_size=5`。两笔理论 fill 均只使用最优 ask 一档、满足 minimum size，并达到 95% 预算成交门槛。

Book state timestamp 为 `1786516572521`（`2026-08-12T06:36:12.521Z`），比完整响应接收时间早 10,972 ms。它表示订单簿状态时间，不应误写成 HTTP 延迟；本次 HTTP batch request duration 为 989 ms。

## 6. Fee 口径

本市场 CLOB fee schedule：

- `rate=0.05`
- `exponent=1`
- `taker_only=true`
- maker/taker base fee metadata：`1000 / 1000 bps`

当前理论计算使用官方一阶公式：

```text
fee = shares × rate × price × (1 - price)
cash_debit = notional + rounded_fee
effective_entry_price = cash_debit / shares
```

Fee 按 5 位小数估算，并在舍入可能导致 cash debit 超过 10U 时仅回退最后一档的极小 shares。若未来 market info 返回其他 exponent，当前研究脚本直接失败，交由后续官方 SDK fill engine 实现，不能猜测新公式。

## 7. 结论与限制

- DATA-005 验收满足：best bid/ask、完整 depth、tick size、minimum size、fee、book hash、book timestamp 和本地接收时间均已保存。
- 双 outcome 的含费 10U 理论 VWAP 可由同一 raw fixture 离线重复计算。
- Best ask 与含费有效价格差异明显；Alpha Bot 不能直接用页面概率或 best ask 代替实际 entry cost。
- 这是理论 taker fill，不是实际订单；没有建模网络变化、撮合竞争、订单精度、实际协议舍入或提交期间盘口移动。
- 生产 Paper/真实执行应使用官方 SDK 的 decimal、fee 和 order-building 实现；本脚本只验证 DATA-005 的数据合同和算例。
