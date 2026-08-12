# Polymarket LOL 市场目录小样本

> Task：DATA-004
>
> 采集与验证日期：2026-08-12
>
> 用途：ProbScout 本地 Research/Paper；不包含钱包、认证或交易调用

## 1. 官方接口与筛选合同

- 官方 API：[Gamma API](https://docs.polymarket.com/api-reference/predictions/overview)
- 官方 endpoint：`GET https://gamma-api.polymarket.com/events/keyset`
- LOL tag：`id=65`，slug `league-of-legends`
- LOL series：`id=10311`
- 快照边界：`2026-08-12T06:00:00Z`
- future：`closed=false`、`end_date_min=<as-of>`、`endDate ASC`
- historical：`closed=true`、`end_date_max=<as-of>`、`endDate DESC`
- 每个 scope 只取第一页 20 events；raw 响应保存在 `data/raw/polymarket_gamma/` 并由 Git 排除

系列赛 Match Winner 不能只按标题猜测。当前实际 event 同时含有系列赛胜者、单局胜者、总局数、让分、击杀和资源类市场，因此候选必须同时满足：

1. event title 匹配 `LoL: <Team A> vs <Team B> (BO1|BO3|BO5) - ...`；
2. market 的 `sportsMarketType` 严格等于 `moneyline`。

`child_moneyline`、`totals`、`map_handicap` 以及其他 series prop 明确排除。

## 2. 可重复命令

默认按当前 UTC 整点建立快照；同一小时内重复运行会复用 cache：

```powershell
.\research\download_polymarket_lol_catalog.ps1
```

重复本次固定快照并禁止任何网络请求：

```powershell
.\research\download_polymarket_lol_catalog.ps1 `
  -AsOfUtc '2026-08-12T06:00:00Z' `
  -Offline
```

显式重新请求两份官方响应：

```powershell
.\research\download_polymarket_lol_catalog.ps1 `
  -AsOfUtc '2026-08-12T06:00:00Z' `
  -Refresh
```

脚本对 future 和 historical 各请求一次，不自动翻页、不隐藏 retry。`Offline` 缺少有效 cache 时直接失败。

## 3. Raw 响应与 fixture

| Scope | Captured at UTC | Raw events | Match Winner candidates | Raw size | Raw SHA-256 |
|---|---|---:|---:|---:|---|
| Future | `2026-08-12T06:23:43.3489222Z` | 20 | 20 | 2,070,600 bytes | `a882a98628b63a1ed9887cc13de3322ae4e54bb0150c9b02640ab61ff3ea3b31` |
| Historical | `2026-08-12T06:23:44.6380404Z` | 20 | 20 | 2,354,197 bytes | `48a8667ceff5d16aaf4aa908f4a0dd2b46c2c834515954bb70f8fa256b207944` |

派生 fixture 包含 40 个 Match Winner 候选，大小 42,081 bytes，SHA-256 为：

`1517133042255ab7b1a953a2ff6ee5d376954a2bdd5dfa112bb2dc99bea04e6f`

离线复跑结果为 `FutureSourceStatus=cached`、`HistoricalSourceStatus=cached`、`FixtureStatus=unchanged`。两份 raw 响应都带 `next_cursor`，因此当前 20 + 20 是受控小样本，不是平台完整 LOL 市场总数。

## 4. ID 示例

### Future candidate

- Event：`812496`
- Title：`LoL: HANJIN BRION Challengers vs Hanwha Life Esports Challengers (BO3) - LCK Challengers League Rounds 3-4 Trial Group`
- Event end date：`2026-08-12T11:00:00.0000000Z`
- Market：`3407649`
- `sportsMarketType`：`moneyline`
- Condition ID：`0x911d914f939ecd4a806233c51faee15ffdc3884e196a97f08a97eb6721b659e9`
- Outcomes：`HANJIN BRION Challengers` / `Hanwha Life Esports Challengers`
- Token 0：`56641059438244450643038459011830916762800460301429810222047727184687822065308`
- Token 1：`26064683978220257461367118007811554823481357879059195079234017982236513092333`
- Snapshot state：`closed=false`、`accepting_orders=true`、`enable_order_book=true`

### Historical candidate

- Event：`819614`
- Title：`LoL: RMD Gaming vs RED Academy (BO3) - Circuito Desafiante Play-In`
- Event end date：`2026-08-12T04:00:00.0000000Z`
- Market：`3438960`
- `sportsMarketType`：`moneyline`
- Condition ID：`0x7a547717728280c4bd3c3e316848066942ca411307c6cb9a88315ad5955b6805`
- Outcomes：`RMD Gaming` / `RED Academy`
- Token 0：`89448963123261266038498596831291905772905176534661850700905338792415767803071`
- Token 1：`57741552571724285144298980056501390892111477506260776542851267284806653410374`
- Snapshot state：`closed=true`、`accepting_orders=false`

## 5. 验证结论与边界

- Future candidates：20，其中 20 个在采集快照时仍接受订单。
- Historical candidates：20。
- 40/40 候选均具有 event ID、market ID、condition ID、两个 outcomes 和两个对应 CLOB token IDs。
- `outcomes[i]` 与 `clobTokenIds[i]` 按相同索引映射；后续不能按队名字母顺序重新排列 token。
- Gamma `endDate` 当前只作为目录时间过滤字段保存，不能在 DATA-004 就断言它始终等于外部赛事源的实际开赛时间；该语义必须在 `DATA-006` 映射任务中交叉核验。
- `closed=false` 属于 event 状态，实际可用性仍须检查 market 的 `closed`、`acceptingOrders` 和 `enableOrderBook`；它不能替代 DATA-005 的真实订单簿检查。
- 本任务只证明目录和核心 ID 可发现、可缓存、可离线重放，不证明盘口可成交、市场映射正确或策略存在 Alpha。
