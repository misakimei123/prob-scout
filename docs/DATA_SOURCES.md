# ProbScout 数据源登记

> 审核日期：2026-08-12
>
> 适用阶段：M1 / 本地 Research 与 Paper 数据可行性
>
> 结论属性：工程使用边界，不替代数据提供方条款或所在地法律要求。

## 1. 当前结论

| Source | 审核日期 | 当前状态 | 当前允许用途 | 当前禁止或待确认事项 |
|---|---|---|---|---|
| Oracle's Elixir | 2026-08-12 | 条件允许 | 本地历史研究、小样本下载、质量核验 | 不公开再分发 raw CSV；整体数据许可未明确前不用于真钱或商业化 |
| Leaguepedia | 2026-08-12 | 条件允许 | 通过公开 Cargo API 获取赛事、赛程、队伍与 roster 小样本 | 禁止 HTML scraper；遵守 attribution、ShareAlike、API 限流及 Fandom Terms；真钱前重新审核 |
| GRID / Riot Esports Data | 2026-08-12 | 当前阻塞 | 只允许阅读公开产品文档和询价 | 无书面商业授权不得调用 API、保存 feed 或用于 betting/trading 产品开发 |
| Polymarket Gamma / CLOB public data | 2026-08-12 | 允许 | 市场发现、订单簿、报价、价格历史、本地 Research/Paper | 不启用认证、钱包和交易 endpoint；不批量公开镜像 API 数据 |
| Riot Developer API | 2026-08-12 | 排除 | 无 | 不申请 Key、不调用 API、不把 Riot API 数据接入 ProbScout |

“允许 Research/Paper”不等于允许真钱。真钱能力仍必须通过长期 Paper、数据授权、账户资格、所在地限制和独立安全评审。

## 2. 通用采集规则

- 只使用公开文档列出的下载页或 API，不逆向网页私有接口。
- 每个 raw 文件保存 source、原始 URL、采集时间、内容 hash 和代码版本。
- raw 数据保存在 `data/raw/`，不进入 Git；第三方原始数据不随代码仓库发布。
- 遵守来源 rate limit，优先缓存和增量采集，不并发轰炸接口。
- 来源条款不明确时，只做最小本地样本验证；不自动升级为真钱、商业或公开再分发用途。
- 接入前和进入真钱评审前重新检查条款；发现条款或访问方式变化时立即停止该来源并更新本文。

## 3. Oracle's Elixir

### 访问与用途

- 数据分发者：Oracle's Elixir / Tim Sevenhuysen；部分底层内容来自 Leaguepedia。
- 入口：[Match Data Downloads](https://oracleselixir.com/tools/downloads)；页面当前将年度 CSV 分发到 Oracle's Elixir 的公开 Google Drive 目录。
- 方式：下载一个年度 CSV 后只导出固定的小时间窗口；`DATA-002` 记录官方文件 ID、raw/sample hash 和字段摘要。旧的 `lol.timsevenhuysen.com` 页面只保留历史数据，不再作为当前入口。
- 用途：历史职业比赛结果、局级统计、基础 Elo/统计特征与数据质量研究。

### License / Terms 判断

- [Oracle's Elixir About](https://lol.timsevenhuysen.com/about/) 只明确说明“部分内容”来自 Leaguepedia，并按 CC BY-SA 3.0 提供。
- 当前未找到明确覆盖整套 downloadable match CSV 的独立 dataset license 或 Terms。不能把“部分内容 CC BY-SA”外推为全部 CSV 均可自由商用或再分发。

### 当前边界

- Research/Paper：允许最小本地样本和派生统计研究，报告必须标注来源与采集日期。
- Model training：仅允许本地 Research 模型训练；artifact 必须记录 source URL、raw hash 和采集日期。
- 真钱/商业：暂不批准。若项目进入真钱评审，需再次检查下载页条款，必要时联系维护者取得书面确认。
- Retention：只保留复现实验所需的本地 raw 文件；来源要求删除或项目 Kill 后按当时条款处理。
- Redistribution：不提交 raw CSV，不提供第三方批量下载镜像；只提交代码、schema、hash、数据质量摘要和不含原始记录的聚合结果。
- 主要风险：单一社区维护来源、历史记录可能修订、不同年份字段可能漂移、整体数据许可不够明确。

## 4. Leaguepedia

### 访问与用途

- 数据所有者/分发者：Leaguepedia contributors 提交内容，Fandom 提供 wiki 与 API 服务。
- 文档：[Leaguepedia API](https://lol.fandom.com/wiki/Help:Leaguepedia_API)。
- 方式：只使用 MediaWiki Cargo 的结构化接口。常规入口为 `api.php?action=cargoquery`；该入口限流时允许使用 Cargo 扩展官方 `Special:CargoExport?format=json`，不抓取渲染后的 HTML 页面。
- 用途：赛事、赛程、队伍、选手、roster、名称别名和系列赛元数据。

### License / Terms 判断

- Leaguepedia 页面声明内容默认按 [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) 提供；Fandom 的 [Licensing](https://community.fandom.com/wiki/Help:Licensing) 要求标明来源、license，并对公开的改编内容遵守 ShareAlike。
- [Fandom Terms of Use](https://www.fandom.com/terms-of-use/) 对未获许可的 scraper、robot 和 automated retrieval 有限制。Leaguepedia 提供专门 API 文档不代表可以忽略服务条款、API 限流或访问控制。

### 当前边界

- Research/Paper：条件允许。`DATA-003` 已用单次小请求验证官方 Cargo JSON export，记录 UTC 响应时间、query/response hash 和限流行为，并确认重复运行复用 cache。
- Model training：只允许使用必要的结构化元数据做本地特征构建；保留页面/API 来源和 CC BY-SA attribution。
- 真钱/商业：CC BY-SA 本身允许商业再利用，但 Fandom 服务访问、Riot IP 和具体 betting 用途没有在该内容许可中获得专门授权；进入真钱前必须重新审核。
- Retention：只缓存实际使用的 API 响应和 hash，不做全站 dump；条款或访问许可变化时停止增量采集。
- Attribution：所有包含 Leaguepedia 派生内容的公开报告注明 `Leaguepedia contributors / CC BY-SA 3.0` 并链接原页面与 license。
- Redistribution：不发布 Fandom 页面镜像；公开派生数据时单独评估是否构成 adaptation，并保留 attribution/ShareAlike metadata。
- 主要风险：社区编辑错误、赛程与 roster 可能滞后、表结构可变化、API 可能严格限流或撤回。

## 5. GRID / Riot Esports Data

### 访问与用途

- 数据权利方：Riot Games；商业分发者：GRID。
- 当前官方入口：[Riot Esports Data - League of Legends](https://riotesportsdata.com/en-us/league-of-legends/)。
- 官方页面说明 GRID 是 LoL 电竞数据的独家分发合作方，提供 live data、fixtures 和 A/V feeds。
- 当前访问方式是联系 GRID 获取商业方案和授权，不存在本项目可以直接假设使用的公开免费 API。

### License / Terms 判断

- Riot 官方页面截至审核日明确写明：live、fixture 和 A/V 数据目前仅通过 GRID 面向商业用途提供，面向社区非商业用途的数据门户仍在规划中。
- 公开可找到的 [GRID Open Data Platform Beta Agreement（2022）](https://cdn.grid.gg/gridgg/GRID_Open_Data_Platform_Agreement_05.04.2022.pdf) 只能作为历史参考，不能证明 2026 年仍存在免费 Open Access。该旧协议也明确要求 betting 等受监管用途先申报并接受 GRID due diligence。

### 当前边界

- Research/Paper：当前阻塞，不申请 Key、不调用 feed。只有取得明确适用于本项目用途的书面条款后才能解锁。
- Model training：当前禁止；未来合同必须明确允许 model training 和 derived data。
- 真钱/商业：必须由 GRID/Riot 的书面合同明确覆盖 betting、prediction market、model training、retention、derived data 和审计要求；普通产品介绍或 API 可访问性不构成许可。
- Retention：当前不获取数据；未来完全按合同的保存与删除条款执行。
- Redistribution：按合同执行；在获得合同前不保存、不展示、不转发任何非公开 feed。
- 主要风险：商业成本、赛区覆盖不完整、用途限制、保密/留存约束、授权可随权利方协议变化。

## 6. Polymarket Gamma / CLOB

### 访问与用途

- 数据/API 分发者：Polymarket；市场规则和 resolution source 由具体 market 定义。
- 文档：[Predictions API Overview](https://docs.polymarket.com/api-reference/predictions/overview) 与 [Market Data Overview](https://docs.polymarket.com/market-data/overview)。
- Gamma：事件、市场、tags、sports metadata 和 token IDs，用于市场发现与匹配。
- `DATA-004` 已确认 LOL `tag_id=65`、series `10311`；系列赛 Match Winner 使用 `sportsMarketType=moneyline`，不能把 `child_moneyline`、totals 或 handicap 当成同一市场。
- CLOB public endpoints：order book、price、spread、tick size、fee rate 和 price history，用于可成交价格研究和 Paper fill。
- `DATA-005` 已确认 Gamma `endDate` 与 CLOB sports `gst` 可能不一致；盘前状态必须以 CLOB `gst` 加赛事源交叉核验，不能仅依赖 Gamma 时间。
- 当前只使用 public REST/WebSocket market data。官方文档说明这些 read endpoints 不需要 API Key、钱包或认证。

### License / Terms 判断

- 使用受 [Polymarket Terms of Service](https://polymarket.com/tos)、API rate limits、市场规则和地理限制约束。
- “公开且无需认证”只说明访问门槛，不等于获得不受限制的批量再分发许可；本项目只保存研究所需快照，不建设公开数据镜像。

### 当前边界

- Research/Paper：允许 read-only Gamma、CLOB market data 和必要的 market WebSocket；保存接收时间、原始响应 hash、市场规则和 quote freshness。
- Model training：允许本地 Research/Paper 的市场基准、校准和策略评估；必须防止时间泄漏，并区分 midpoint、last price 与可成交 ask。
- 真钱：当前禁止认证、签名、下单、取消、allowance、bridge 和钱包注入。进入真钱评审时必须再次核对 [Geographic Restrictions](https://docs.polymarket.com/api-reference/geoblock)、Terms、账户资格和所在地法律。
- Retention：只保存复现实验和 Paper 审计所需的市场快照；不建立面向第三方的历史数据服务。
- Redistribution：不发布完整订单簿/价格历史镜像；报告只输出支持研究复核所需的市场 ID、时间、摘要指标和小型 fixture。
- 主要风险：API 版本与分页变化、市场规则解释、薄订单簿、历史 price 不等于历史可成交 ask、地区限制动态变化。

## 7. 为什么不接入普通 Riot Developer API

- [Riot LoL Developer Policy](https://developer.riotgames.com/docs/lol) 的一般政策写明 `No cryptocurrencies or no blockchain`；Monetization Policy 还规定产品不能包含 betting/gambling functionality。
- ProbScout 的目标市场是 Polymarket，并保留未来真钱评审路径。即使当前只做免费本地 Research，也不能据此推定 Riot 会批准其 API 数据进入该交易链路。
- 普通 Riot API 主要面向玩家账号、Match、Tournament 和静态游戏数据，并不是职业赛事官方实时 feed 的替代品。

因此本项目执行以下硬约束：

- 不创建或保存 Riot Developer API Key；
- 不调用 Riot Developer API、League Client API 或未公开的 LoL Esports 网页接口；
- Data Dragon 若未来只用于 patch/champion/item 静态字典，必须建立独立 source entry 并重新审核，不能借此绕过当前排除决定；
- 若需要官方职业赛事 telemetry，只能走 GRID/Riot 明确书面授权路径。

## 8. 下一步准入

| Task | 是否可开始 | 前置限制 |
|---|---|---|
| `DATA-002` Oracle's Elixir 小样本 | 是 | 只取小窗口；保存 URL/hash；不提交 raw CSV |
| `DATA-003` Leaguepedia 小样本 | 已完成 | 只用 Cargo API/CargoExport；低频请求；记录限流；不得 HTML scraping |
| `DATA-004` Polymarket 市场目录 | 已完成 | 官方 Gamma API read-only；future/historical raw 与派生 fixture 已缓存 |
| `DATA-005` Polymarket 订单簿 | 已完成 | 双 token 完整 depth、CLOB market info、含费 10U VWAP 与离线重放已验证 |
| `DATA-006` 统一 Event 和别名 | 是 | Leaguepedia/Gamma/CLOB `gst` 必须保留各自 ID 与时间证据；冲突不得静默合并 |
| GRID 数据接入 | 否 | 等待明确商业授权与用途许可 |
| Riot Developer API 接入 | 否 | 当前项目明确排除 |
