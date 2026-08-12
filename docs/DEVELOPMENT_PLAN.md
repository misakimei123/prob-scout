# ProbScout 详细开发计划

> 状态：Draft v4
>
> 制定日期：2026-08-12
>
> 当前题材：League of Legends（LOL）职业比赛胜负预测
>
> 当前阶段：Research First，先在本地完成 Paper 与真实下单链路验证，再条件性进入小资金实盘；暂不部署长期 VPS

> 执行清单：[TASK_BREAKDOWN.md](./TASK_BREAKDOWN.md)

## 0. 文档使用方式

ProbScout 只维护两份核心计划文档，不复制 IronPilot 的多层治理体系：

- 本文件回答“项目是什么、为什么做、实验规则是什么、何时继续或停止”，属于稳定计划；
- `TASK_BREAKDOWN.md` 回答“下一步做什么、依赖什么、怎样算完成”，并维护实际进度；
- 用户最新明确决定优先于文档；发生方向变化时先修改本文件，再同步任务清单；
- 普通开发只更新任务状态和证据，不因完成一个 Task 重写整份计划；
- 不为普通可回退选择创建 ADR。只有真钱权限、数据许可、实验口径等难回退决策才单独记录。

任务的最小完成闭环是：确认依赖与边界 → 检查成熟开源库 → 做最小实现 → 运行窄范围验证 → 记录结果。Task 完成不代表阶段 Gate 自动通过；Gate 必须根据该阶段累计证据单独作出 `Go`、`Conditional Go` 或 `Kill` 判断。

## 1. 项目定义

ProbScout 是一个可扩展到多类事件的概率预测与预测市场交易验证系统。系统先使用独立于市场价格的赛前数据生成经过校准的事件结果概率，再将该概率与预测市场的真实可成交报价比较，通过历史研究、实时 Paper、受控真实下单和小资金实盘判断是否存在可重复、扣除执行成本后仍为正的优势。

LOL 是第一个验证题材，但核心领域模型不绑定 LOL，也不绑定 Polymarket。未来可以增加其他电竞、体育或非体育事件适配器，但不得在 LOL 研究尚未通过 Gate 前提前扩张题材。

### 1.1 第一版要回答的问题

1. 只要模型判断某队是高概率获胜方就买入，长期是否有正收益？
2. 只有模型概率高于 Polymarket 可成交概率足够多时才买入，是否更赚钱、更稳定？
3. 模型是否真的比简单 Elo、历史胜率和市场概率更准确？
4. 回测优势能否在实时、不可回看的 Paper Trading 中复现？
5. Paper 假设与真实下单之间存在多大的成交、fee、延迟和拒单偏差？
6. 优势是否足以覆盖 spread、slippage、fee、延迟和失败成交？

### 1.2 第一版明确不做

- 不在完成本地 Paper 稳定性验证和真实下单 smoke test 前直接运行小资金实盘。
- 不使用 Riot Developer API 为交易系统提供数据。
- 不做盘中预测、盘中开仓、加仓、减仓、止损或主动退出；比赛开始后只等待结算。
- 不运行本地大型语言模型。
- 不让 GPT 直接生成最终交易概率。
- 不覆盖 Map Winner、First Blood、击杀数等衍生市场。
- 不同时扩展 CS2、Dota 2、Valorant 或传统体育。
- 不开发复杂前端、移动端或公开 SaaS。
- 不部署高可用集群、Kubernetes 或微服务体系。
- 不把 VPN、代理或更换服务器地区作为规避平台访问限制的系统能力。

### 1.3 个人项目执行模式

ProbScout 当前是单人、低成本、研究优先的个人项目，不按企业系统建设。默认选择最短可验证路径：

- 一个 Git 仓库；
- 一个 Rust 可执行程序；
- 一个本地 SQLite 数据库；
- 一套简单配置文件和环境变量；
- Python 仅在统计研究明显更高效时使用；
- CLI 和 Markdown/CSV 报告优先，不开发 Dashboard；
- 先在本地连续运行 Paper，再在满足准入条件时完成极小额真实下单 smoke test 和小资金实盘；
- 只有本地链路已经稳定且确实需要长期无人值守时才购买 VPS；
- 不为了“架构完整”提前设计多服务、消息队列、插件系统和通用框架；
- 不要求 Jira、Sprint、PR 模板、复杂发布流程或大量过程文档；
- 每完成一个能独立验证的小任务，就运行对应的最小测试并更新任务清单。

现有 Gate 不是企业审批流程，而是防止个人研究在数据不足、过拟合或模拟成交失真时继续投入资金。Gate 只保留能改变“继续、回退、停止”决定的证据。

### 1.4 不能因个人项目而降低的底线

- 禁止未来数据进入历史预测。
- 禁止用 midpoint 冒充真实买入价。
- 禁止回写或删除亏损交易来改善结果。
- 禁止两个策略共享资金账本。
- 禁止把 Paper fill 描述成真实成交。
- 禁止将 API Key、钱包私钥或 `.env` 提交 Git。
- 禁止在数据用途、账户资格和平台访问资格不明确时启用真实下单。
- 禁止在 Git 跟踪的文档、fixture、日志样例或可公开报告中记录实际本金、钱包地址、私钥、账户标识、所在地和网络规避方案。

## 2. 已锁定的实验合同

以下规则在正式 Paper 开始前写入带版本号的配置快照；开始后不得回写历史交易。真实资金规模、钱包和账户信息只存在于 Git 忽略的本地 secret/config 中，计划文档只记录抽象风险边界。

### 2.1 研究对象

- 市场：职业 LOL 系列赛最终胜者（Match/Series Winner）。
- 阶段：Prematch。
- 决策时间：官方计划开赛时间前 15 分钟，即 `T-15m`。
- 决策信息：仅允许使用 `T-15m` 之前已发布、已采集并带时间戳的信息。
- 退出规则：`HoldToResolution`，第一版全部持有到 Polymarket 市场结算。
- 初始虚拟资金：每个策略 500U，完全独立记账。
- 单笔预算：每次最多从对应 Paper Account 扣除 10U，包含模拟 fee。
- 真实执行：两个策略继续独立记账，使用隔离的小资金实验预算和配置化固定小额单笔；实际数值不写入仓库。
- Shadow 对照：每个真实 TradeIntent 同时生成同输入的 Paper order，用于量化真实成交与模拟成交偏差。
- 同一策略、同一市场：最多建立一次初始仓位，不追单、不摊平。

如果比赛实际开赛时间变化，系统必须同时保留原计划时间、最新计划时间和真实开赛时间。只有在真实比赛尚未开始且市场仍接受订单时，`T-15m` 快照才有效。

### 2.2 Prediction 合同

每次预测至少保存：

- `prediction_id`
- `model_version`
- `feature_version`
- `event_id`
- `information_cutoff_at`
- `generated_at`
- 双方原始概率
- 双方校准后概率
- 输入数据来源与数据版本
- 缺失字段和降级标记

同一 `prediction_id` 必须同时提供给两个策略。策略无权修改概率，只能根据概率和市场报价决定是否产生 TradeIntent。

### 2.3 Market Quote 合同

不得把页面展示的 midpoint 或 last trade 当成买入成本。每个报价快照至少保存：

- `condition_id` 和 outcome `token_id`
- best bid / best ask
- 多档 bid / ask depth
- spread
- tick size
- minimum order size
- fee enabled 和当时 fee 参数
- Polymarket 服务器时间与本地接收时间
- order book hash（若接口提供）

对固定 10U 预算遍历 ask depth，计算实际可获得 shares、VWAP、fee 和未成交金额：

```text
cash_debit = filled_notional + taker_fee
effective_entry_price = cash_debit / filled_shares
```

只有当 `cash_debit <= 10U` 且成交量达到配置的最低比例时，Paper fill 才有效。默认要求至少模拟成交计划预算的 95%；否则标记为 `RejectedInsufficientLiquidity`，不得按 best ask 假装全部成交。

## 3. 双策略定义

### 3.1 Threshold Strategy

Threshold Strategy 是高概率买入基准，不判断市场是否错价。

初始候选规则：

```text
if calibrated_probability >= 0.75
and quote is executable
and risk check passes:
    create TradeIntent with max_cash_debit = 10U
```

`0.75` 是预注册候选阈值。历史训练与 validation 可以比较少量预先声明的阈值，但最终阈值必须在 untouched test 和实时 Paper 之前锁定。

### 3.2 Edge Strategy

Edge Strategy 只在模型概率相对真实成交成本存在足够净优势时入场。

```text
conservative_probability = calibrated_probability - uncertainty_buffer
net_edge = conservative_probability - effective_entry_price

if calibrated_probability >= min_probability
and net_edge >= min_edge
and quote is executable
and risk check passes:
    create TradeIntent with max_cash_debit = 10U
```

`min_probability`、`min_edge` 和 `uncertainty_buffer` 只能使用训练集和 validation set 选择。禁止在最终 test 或实时 Paper 结果出现后反复调参并覆盖旧版本。

### 3.3 公平比较要求

- 两个策略读取同一个 Prediction。
- 两个策略读取同一个时间点的 Market Quote。
- 两个策略都使用相同的最大现金扣款规则。
- 两个策略第一版都使用 `HoldToResolution`。
- 两个策略分别维护现金、持仓、已实现 PnL 和权益曲线。
- 同一比赛两个策略可以独立产生 Paper fill，但不得合并账本。
- 报告必须同时展示全部候选机会和实际成交机会，防止只报告胜出的交易。

## 4. 基准模型与预测模型

### 4.1 必须实现的基准

1. `ConstantBaseline`：按训练数据总体胜率或双方对称 50% 输出。
2. `EloBaseline`：仅使用比赛发生前可得的历史结果更新 Elo。
3. `MarketBaseline`：使用同一决策时点的市场概率；评估时明确使用 midpoint 或去价差后的概率，交易成本仍使用 ask。

### 4.2 第一版主模型

优先建立可解释的赛前统计模型，候选输入包括：

- 队伍赛前 Elo 与赛区强度修正
- 近期比赛结果，但必须设置时间衰减
- 对手强度
- BO3 / BO5 赛制
- Patch 版本
- 赛区与赛事级别
- 选边信息（仅当 `T-15m` 前可靠可得）
- 已确认首发阵容与 roster stability
- 跨赛区比赛和国际赛事标记

第一版不使用会造成高维过拟合的大量局内统计。先比较 logistic regression、Elo-logistic 和一个受控的 tree model；模型选择以时间外推表现和 calibration 为主，不以训练集 accuracy 为主。

### 4.3 概率校准

模型必须在独立 calibration window 上使用 Platt scaling 或 isotonic regression。不得在最终 test set 上拟合校准器。

输出必须保留：

- raw probability
- calibrated probability
- calibration model version
- uncertainty estimate

## 5. 数据源与授权策略

### 5.1 数据职责边界

ProbScout 不把 Polymarket 价格同时当作“独立胜率预测”和“被比较的市场价格”。两类数据承担不同职责：

- LOL 基本面数据生成 `model_probability`：历史比赛、对手强度、赛区、赛制、Patch 和赛前 roster 等；
- Polymarket 数据提供 `market_probability` 与执行条件：市场发现、ask、depth、spread、fee、市场状态和结算；
- `MarketBaseline` 可以使用 Polymarket 概率作为基准，但不能冒充独立 Alpha 模型；
- 如果某个实验只使用 Polymarket 数据，它必须明确标记为市场微结构或市场跟随实验，不得与 LOL 基本面 Alpha 混为一谈。

因此，Oracle's Elixir、Leaguepedia 等来源虽然主要在研究/训练阶段使用，只要其派生模型参与真实 TradeIntent，就仍需在进入真实执行前复核相应用途；Polymarket 数据公开可读也不自动等于真实下单资格已经满足。

### 5.2 研究与运行数据

- Oracle's Elixir：历史职业比赛与局级统计。
- Leaguepedia：赛事、赛程、队伍、选手和 roster 元数据。
- GRID / Riot 官方电竞数据：当前仅通过商业授权提供；在获得书面许可前不接入，只保留为未来官方数据核验候选。
- Polymarket Gamma API：事件和市场发现。
- Polymarket CLOB API：赛前订单簿、报价、价格历史、Paper 与真实执行核验数据。

### 5.3 禁止路径

普通 Riot Developer API 的一般政策禁止 crypto/blockchain，Monetization Policy 还禁止产品包含 betting/gambling 功能。ProbScout 面向 Polymarket，并包含条件性真实执行，因此普通 Riot API 与项目用途存在直接冲突或重大授权不确定性，不得接入其 API Key 或数据。第一版不做盘中预测，不需要职业赛事盘中 telemetry；GRID/Riot 只保留为未来独立项目的授权候选，不属于当前路线。

### 5.4 数据许可 Gate

每个数据源在接入前建立 Source Registry，至少记录：

- source name 和 URL
- 数据所有者与分发者
- 访问方式
- license / terms URL
- 允许用途
- 是否允许模型训练
- 是否允许 Paper Trading
- 是否允许真钱或商业用途
- attribution 要求
- retention 和 redistribution 限制
- 最后审核日期

用途不明确时，默认只允许本地研究，不允许真实执行、公开再分发或商业化。历史数据不在运行时实时请求，不代表其训练得到的模型与真实交易用途自动脱钩。

### 5.5 权威参考

- [Riot Developer Policy](https://developer.riotgames.com/docs/lol)
- [Riot 官方电竞数据](https://riotesportsdata.com/league-of-legends/)
- [GRID League of Legends Data Portal](https://grid.gg/get-league-of-legends/)
- [Polymarket API Overview](https://docs.polymarket.com/api-reference/predictions/overview)
- [Polymarket Orderbook](https://docs.polymarket.com/trading/orderbook)
- [Polymarket Sports Orders](https://docs.polymarket.com/trading/orders/create)
- [Polymarket Early Exit](https://help.polymarket.com/en/articles/13364247-can-i-sell-early)

## 6. 历史数据真实性分级

历史研究必须根据市场数据质量分级，禁止把弱证据包装成可成交回测。

### Grade A：可成交回测

决策时点存在完整或足以重建 10U fill 的 order book snapshot，并有当时 fee 参数。可以计算 spread、depth、slippage 和成交失败。

### Grade B：近似可成交研究

有决策时点 best bid/ask，但没有完整 depth。只能对小额订单使用保守 slippage stress，结论必须标记为近似。

### Grade C：信号研究

只有 midpoint、last trade 或稀疏 price history。只能研究预测准确度和方向性 Edge，不得声称历史 PnL 可真实执行。

如果500场样本大部分只能达到 Grade C，则项目不直接 Kill，但必须跳过“历史可成交盈利已证明”的结论，转入更长的实时 Paper 数据采集。

## 7. 数据划分和防泄漏规则

- 只能按时间切分，禁止随机打散比赛。
- 建议使用 expanding-window walk-forward。
- 训练、validation、calibration 和 final test 时间段完全分离。
- 同一系列赛中的多个小局不得跨训练和测试边界。
- 任何在比赛结束后才确定的信息都不得进入赛前特征。
- roster、Patch、赛程和市场报价必须使用当时版本，而不是当前修订后的页面状态。
- 数据清洗规则必须先在训练集确定，然后统一应用到后续数据。
- 所有数据集生成脚本输出 manifest、row count、时间范围和内容 hash。

最低目标是500场 eligible series，但样本数量不是自动通过条件。最终判断依赖置信区间、跨时间稳定性和执行成本压力测试。

## 8. 系统架构

第一版采用简单单体程序，不采用微服务、企业级分层或预先设计的插件框架。生产运行时以 Rust 为主；研究和模型训练允许 Python。只有在出现第二个真实实现、文件已经难以维护或测试明确需要时，才继续拆分模块。

```text
prob-scout/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── db.rs
│   ├── events.rs
│   ├── polymarket.rs
│   ├── prediction.rs
│   ├── strategy.rs
│   ├── paper.rs
│   ├── settlement.rs
│   └── report.rs
├── migrations/
├── research/
│   ├── pyproject.toml
│   └── notebooks/
├── config/
├── tests/
│   └── fixtures/
├── docs/
└── data/                 # 本地数据，不提交 Git
```

### 8.1 Rust 技术选择

- Tokio：异步运行时和调度。
- reqwest：HTTP API。
- tokio-tungstenite：WebSocket。
- serde：序列化。
- SQLx：SQLite 访问和 migration。
- rust_decimal：价格、资金和 PnL，禁止使用 `f32/f64` 表示现金。
- tracing：结构化日志。
- clap：研究与运维 CLI。
- chrono 或 time：UTC 时间处理。

版本号在实际初始化时根据当前稳定版确认，不在计划文档中硬编码。

### 8.2 Open Source First 硬约束

除 ProbScout 自身领域逻辑外，默认必须优先采用成熟开源库，不允许为了练习、控制感或“以后也许会需要”而重写通用基础设施。

开发任何通用能力前必须按以下顺序处理：

1. 查找官方 SDK 或官方推荐库。
2. 查找 crates.io、PyPI 和 GitHub 上维护中的成熟开源库。
3. 比较 license、维护状态、文档、测试、平台支持和依赖体积。
4. 选择能满足当前需求的最小库，不追求功能最多。
5. 存在满足当前边界的成熟库或 SDK 时必须采用；不得以“代码不多”“方便测试”或“以后更灵活”为理由重写同类能力。
6. 如果所有候选都无法满足必要边界，先在任务清单记录缺口、候选和最小替代范围，并由用户明确确认后再写最小适配代码。

以下能力禁止从零手写：

- HTTP、WebSocket、TLS 和连接池；
- JSON/CSV/Parquet 序列化；
- SQLite driver、migration 和连接管理；
- Decimal 金额计算；
- CLI 参数、配置解析和日志框架；
- 日期时间、时区和定时调度基础能力；
- 通用 retry/backoff；
- 统计指标、概率校准和常见机器学习算法；
- 图表和表格导出；
- Polymarket 已有 SDK 能正确覆盖的签名或订单协议。

允许自行实现的内容限于项目特有逻辑：

- 多数据源比赛身份匹配；
- `T-15m` 特征快照规则；
- Threshold Strategy 和 Edge Strategy；
- 10U Paper fill 的业务约束；
- 项目风险规则；
- Polymarket 市场规则与 LOL 结果的结算映射；
- 为现有库补充的极薄适配代码。

如果开源库只缺少很小能力，应提交上游、组合现有 API 或编写小型适配器，不得复制整套库。优先选择 MIT、Apache-2.0、BSD 等宽松许可；其他许可在引入前单独检查兼容性。禁止直接复制无明确 license 的代码。

引入外部库时必须检查其默认 timeout、自动 retry、遥测、分页和数据外发行为。开源优先不等于盲目信任依赖；任何会改变请求次数、成本、时间点或回测语义的默认行为都必须显式配置或关闭。

每次新增依赖只需在 `docs/DEPENDENCIES.md` 记录一行：用途、选用库、license、选择原因和替代方案。该文件是轻量清单，不写长篇 ADR。

### 8.3 Python 边界

Python 仅负责：

- 数据探索和数据质量报告；
- 模型训练、walk-forward 和 calibration；
- 统计检验与可视化；
- 导出不可变模型 artifact。

Rust 负责：

- 实时采集；
- 特征快照和模型推理；
- 策略、风险、Paper/真实执行编排和账本；
- 调度、恢复、结算和报告数据生成。

Python 不直接写实时账本，Rust 不在运行时自动重新训练模型。

## 9. 核心数据模型

SQLite 第一版至少包含以下表：

| 表 | 用途 |
|---|---|
| `events` | 统一 LOL 系列赛身份、时间和状态 |
| `event_aliases` | 各数据源队伍、赛事和比赛 ID 映射 |
| `source_records` | 原始数据引用、采集时间和 hash |
| `feature_snapshots` | 信息截止时点的不可变特征 |
| `predictions` | 模型概率和版本 |
| `market_snapshots` | 订单簿与费用快照 |
| `trade_intents` | 策略希望进行的交易 |
| `paper_orders` | 模拟订单和拒绝原因 |
| `paper_fills` | 模拟成交明细、VWAP 和费用 |
| `live_orders` | 脱敏后的真实订单生命周期、平台 order id hash 和状态 |
| `live_fills` | 脱敏后的真实成交、价格、数量和费用 |
| `execution_pairs` | 真实订单与同输入 Shadow Paper 订单的关联 |
| `positions` | 各策略独立持仓 |
| `settlements` | 市场规则、结果和结算状态 |
| `ledger_entries` | 双分录或可审计资金流水 |
| `model_registry` | 模型、特征和 calibration 版本 |
| `strategy_configs` | 已锁定的策略配置和 hash |

所有业务写入必须具备幂等键。服务重启后不得重复预测、重复下单或重复结算。数据库和日志不得保存钱包私钥、完整鉴权材料或无需审计的账户身份信息。

## 10. Paper Execution 规则

Paper Trading 不能简单记录“信号价格”，必须模拟订单生命周期：

1. 在决策时间冻结 Prediction 和 Market Quote。
2. Risk Manager 检查余额、事件暴露和 quote 新鲜度。
3. 从 ask 最低档向上遍历，直到预算耗尽或流动性不足。
4. 使用当时 fee 参数计算现金扣款。
5. 保存每个价位的模拟 fill。
6. 无法满足最低 fill ratio 时拒绝交易。
7. 市场关闭、quote 过期、比赛已开始或映射不确定时拒绝交易。
8. 市场结算后依据市场规则结算，不只依赖比赛标题或比分。

默认 quote freshness 上限先设为5秒，待接口延迟实测后锁定。历史回测的 freshness 规则必须与实时 Paper 区分。

### 10.1 真实执行模式

执行层必须显式区分三个互斥模式，默认且无配置时只能进入 `paper`：

1. `paper`：只生成模拟订单，不加载签名材料；
2. `live_smoke`：只允许人工选择的单个测试意图和极小额订单，用于验证鉴权、签名、下单、回报、结算和对账；
3. `live_small`：仅在本地 Gate 通过后运行两个策略的小资金盘前实验，同时保留 Shadow Paper 对照。

从 `paper` 升级到真实执行必须同时满足人工总开关、环境准入、数据用途复核、模型/策略配置 hash 锁定、资金上限、quote 新鲜度和账本健康检查。任何条件缺失都 fail closed。真实下单使用带价格保护的订单，不允许无限追价；订单被拒绝、部分成交或状态不明时不得伪造补单。

## 11. Risk Manager

### 11.1 Paper 阶段

- 每个策略初始500U。
- 单笔现金扣款不超过10U，即初始权益的2%。
- 同一事件单策略最多一个方向、一个初始仓位。
- 同时开放仓位总成本不超过当前权益的20%。
- 同一赛事日新增风险不超过当前权益的20%。
- 数据或市场状态异常时 fail closed，不产生交易。

### 11.2 小资金实盘阶段

- 真实资金与日常资金隔离，并被视为可全部损失的实验预算；实际金额不写入仓库。
- 两个策略使用独立虚拟子账本和独立暴露上限，但可以由同一个最小权限执行账户完成订单。
- 单笔使用配置化固定小额，不因连续盈利或亏损自动放大。
- 同时开放成本、单日新增成本、单策略累计亏损和全局累计亏损均设置硬上限。
- 每个真实订单同时运行 Shadow Paper；报告必须区分策略差异与执行差异。
- 不允许 martingale、亏损加仓或为了增加 Opportunity Rate 降低阈值。
- 达到任一亏损、对账、数据、资格或执行异常门槛时停止新开仓；已有持仓按 `HoldToResolution` 管理。

真实资金规模、固定单笔和熔断数值在进入该阶段前写入 Git 忽略的本地配置快照，不继承 Paper 的公开示例金额，也不在报告中公开。

## 12. 评估指标

### 12.1 预测质量

- Brier Score
- Log Loss
- Calibration curve
- Calibration intercept / slope
- Expected Calibration Error
- 相对 EloBaseline 和 MarketBaseline 的 skill improvement

Accuracy 和 Win Rate 只能作为辅助指标；高概率队策略天然具有高 Win Rate，但仍可能因买价过高而亏损。

### 12.2 交易质量

- Net PnL
- Net ROI
- Max Drawdown：金额、比例和持续时间
- Opportunity Rate
- Fill Rate / Reject Rate
- 平均 spread、slippage 和 fee
- 资金利用率和 capital-hours utilization
- 按预注册 Edge bucket 的收益
- 按赛区、Patch、BO3/BO5 和市场流动性分段的收益

### 12.3 不确定性

- 对系列赛和队伍相关性使用 block bootstrap。
- 报告95%置信区间，不只报告点估计。
- 报告最好、基准和压力场景。
- 压力场景至少增加1–3个 tick slippage，并模拟部分成交和一定比例失败成交。

## 13. 阶段计划与 Gate

本节编号与 `TASK_BREAKDOWN.md` 完全一致。M0 是最小工程骨架；Gate 0 从 M1 数据可行性结束时开始。完整 Task ID、直接依赖和逐项验收只在任务清单维护。

### M0：仓库与最小运行骨架

目标：建立能编译、能读取配置、能写 SQLite 的最小 Rust 单程序，不实现策略。

#### 工作项

- 补全 `.gitignore`；
- 初始化单 package、单 binary Rust 项目；
- 建立轻量开源依赖清单；
- 接入配置、日志和 SQLite migration；
- 固定最小 format、check 和 test 命令。

#### M0 完成检查

- Windows 本地能够编译和运行；
- 配置、日志和 SQLite smoke test 通过；
- 通用能力已有开源库选择记录；
- 尚未引入策略、真钱交易或 VPS 部署代码。

预计投入：2–4天。

### M1：数据可行性

目标：证明所需数据能获取、能授权使用、能正确匹配，并明确历史市场数据等级。

#### 工作项

- 初始化 Rust 项目和基础 CI。
- 建立 Source Registry。
- 下载一小段 Oracle's Elixir 历史数据。
- 读取 Leaguepedia 赛程、队伍和 roster 示例。
- 读取 Polymarket Gamma 市场和 CLOB quote 示例。
- 设计统一 `EventId` 和别名映射规则。
- 人工核验至少50个比赛与市场映射。
- 调查历史 bid/ask/depth 可获得程度并给出 Grade A/B/C 比例。
- 建立数据 manifest 和不可变 raw snapshot 规则。

#### 交付物

- 可重复的数据下载命令。
- 数据字典和 Source Registry。
- 50场映射质量报告。
- 历史市场数据真实性分级报告。
- Go / Conditional Go / Kill 决策。

#### Gate 0

通过条件：

- 比赛与市场映射人工抽检准确率达到100%；
- 关键字段 completeness 达到95%以上，或有明确降级规则；
- 数据用途满足本地研究；
- 至少能形成 Grade C 信号研究，且实时 CLOB 可采集真实 order book。

Kill 条件：

- 无法稳定识别目标 LOL 市场；
- 结果和市场结算规则无法可靠对齐；
- 所有候选数据源都不允许该研究用途；
- 实时可成交报价无法稳定采集。

预计投入：1–2周。

### M2–M3：历史数据集与概率预测研究

目标：在不考虑复杂交易执行前，判断模型是否具有概率预测价值。

#### 工作项

- 构建 clean event dataset。
- 实现 Constant、Elo 和 Market baselines。
- 实现赛前统计模型和 calibration。
- 进行 expanding-window walk-forward。
- 输出整体与分段的 Brier、Log Loss 和 calibration。
- 建立 feature leakage tests。
- 建立500场以上 eligible series 的最终测试集；如果覆盖不足，延长时间范围而不是降低质量标准。

#### Gate 1

继续条件：

- final test 上模型相对 EloBaseline 至少表现不劣；
- 相对 MarketBaseline 至少一个主要 scoring rule 有正 improvement，且不是单一赛区贡献；
- calibration 没有明显系统性过度自信；
- 结果在多个时间窗口方向一致。

Kill 或回退条件：

- 模型在 final test 上稳定劣于 Elo 和市场；
- 所谓优势只来自数据泄漏、少数异常比赛或事后分桶；
- 不同 Patch/赛区的结果完全不可迁移且样本不足以单独建模。

预计投入：2–4周。

### M4：历史双策略研究

目标：比较 Threshold Strategy 和 Edge Strategy；证据强度取决于历史市场数据 Grade。

#### 工作项

- 在 training/validation 上选择有限的阈值候选。
- 锁定最终策略配置和 hash。
- 回放两个独立 Paper Account。
- 计算 fee、spread、slippage stress 和失败成交。
- 输出分段收益、回撤和置信区间。
- 明确结论属于可成交回测、近似研究还是信号研究。

#### Gate 2

Grade A/B 数据下继续条件：

- 至少一个策略的压力场景净收益为正；
- 收益不是由单一队伍、赛事或 Edge bucket 主导；
- bootstrap 后结论仍有实际意义；
- Edge Strategy 相对 Threshold Strategy 的差异能够被解释和复核。

Grade C 数据下不以 ROI 作为通过证据，只要预测信号仍有价值，就进入实时 Paper 收集真实 order book。

预计投入：1–2周。

### M5：本地实时 Paper 与真实执行验证

目标：先在本地用真实行情验证完整自动化链路，再以受控 smoke test 验证真实鉴权、签名、下单、成交回报和对账；本阶段不要求 VPS。

#### 工作项

- 自动发现未来 LOL 市场。
- 自动匹配赛事和 Polymarket token。
- 在 `T-15m` 生成不可变 feature snapshot 和 Prediction。
- 采集 order book 并生成两个策略决策。
- 模拟 depth-aware fill 和 fee。
- 重启后恢复调度和持仓。
- 自动等待市场结算并写入账本。
- 生成每日数据完整性和策略报告。
- 对首批机会和 Paper fills 进行逐笔人工复核。
- 验证 `paper`、`live_smoke` 和 `live_small` 模式无法被意外混用。
- 在准入检查通过后，以极小额单一测试意图完成真实下单 smoke test。
- smoke test 对账无误后，在本地启动双策略小资金盘前实验，并为每笔真实订单生成 Shadow Paper 对照。

#### Gate 3-A：本地 Paper 稳定性

- 在本地连续运行至少3–7天，覆盖断线、重启和至少一个完整结算周期；
- 无重复交易或重复结算；
- Prediction、Quote、TradeIntent、Fill、Settlement 可完整追踪；
- 关键采集任务成功率达到99%；
- 人工复核账本与规则一致；
- 默认配置仍为 `paper`，缺少显式人工总开关时无法加载真钱凭证。

#### Gate 3-B：本地真实执行准入

- 账户、平台访问资格、数据用途和执行环境检查通过；
- wallet secret 只存在于 Git 忽略的本地 secret/config，日志与数据库均无泄漏；
- `live_smoke` 的下单、成交回报、fee、结算和账本全部对平；
- 重试、超时或重启不会产生重复真实订单；
- 双策略独立账本、全局暴露上限、累计亏损熔断和人工总开关通过故障注入测试；
- 未通过任一项时保持 Paper，不因追求刺激降低门槛。

预计投入：开发2–3周，随后先运行数天本地 Paper，再按 Gate 逐级解锁真实执行。

### M6：可选 VPS 持续运行

目标：只有本地 Paper、真实下单 smoke test 和小资金双策略链路稳定后，才把同一单体程序迁移到 VPS，用于长期无人值守采样；VPS 不是策略验证或真实下单的前置条件。

#### 推荐 VPS

- Ubuntu LTS
- 2 vCPU
- 2 GB RAM
- 30–40 GB SSD
- 每日异地备份 SQLite 和配置快照

如果同机运行 Python 分析任务，升级到4 GB RAM。模型训练仍优先在本地进行。

#### 运维要求

- systemd 托管和自动重启；
- UTC 时钟同步；
- 健康检查和断线重连；
- SQLite WAL、定期 checkpoint 和备份恢复演练；
- 磁盘、内存、任务延迟和采集失败告警；
- secrets 只通过环境变量或 secret file 注入；
- 是否在 VPS 启用真实执行必须重新完成人工准入；默认只迁移 Paper/Shadow 采样。
- 如确需 VPS 真实执行，secret 必须使用最小权限 secret file 注入，并重新核对执行环境的平台访问资格。

#### Gate 4

扩大运行时长或资金风险前至少满足：

- 连续运行1–3个月；
- Paper 与真实 fill 累积到足以估计执行偏差和策略方差；不把固定笔数当作自动通过条件；
- 至少一个策略净 ROI 为正，95%置信区间和压力场景足够支持继续试验；
- Max Drawdown 在预先声明的风险预算内；
- 结果不依赖单一赛区或短时间窗口；
- 历史与实时表现差异得到解释；
- 数据用途、账户资格和平台访问资格持续有效。

否则停止新增真实仓位、继续 Paper 或 Kill，不因为机器人需要交易而降低阈值。

### M7：GPT 非结构化信息增强（Deferred）

目标：验证 LLM 是否能通过结构化事件抽取提升概率，而不是直接让 LLM 报胜率。

```text
官方公告 / 可靠新闻 / 采访
        ↓
LLM 结构化提取
        ↓
缺席、首发、Patch 适应、健康等受控字段
        ↓
统计模型重新计算概率
```

必须记录模型版本、prompt hash、来源 URL、发布时间、抓取时间、信息截止时间和结构化输出。先做 shadow prediction，不直接影响策略；只有预注册 A/B 评估显示 OOS Brier/Log Loss 改善，才允许进入正式模型。

盘间和盘中预测、止损及动态退出不属于当前路线。若未来重新提出，应作为独立项目重新审核数据、模型和执行合同，不在本计划中保留伪待办。

## 14. 测试计划

### 单元测试

- 概率和阈值边界；
- 10U depth-aware fill；
- fee 和 decimal rounding；
- 部分成交与流动性不足；
- 风险上限；
- PnL、ROI 和 drawdown；
- 结算、void、50/50 和取消场景；
- 时间截止和过期 quote；
- 幂等键与重复消息。

### 集成测试

- Gamma market → token → CLOB book 完整发现链路；
- esports event → market mapping；
- Prediction → Strategy → Risk → Fill → Ledger；
- TradeIntent → live order → fill/reject → Shadow Paper → reconciliation；
- 服务重启后的恢复；
- WebSocket 断线、乱序和重复事件；
- SQLite migration、备份与恢复。

### 回放测试

保存脱敏的市场和赛事 fixtures，确定性重放同一输入必须得到相同 Prediction、TradeIntent 和 Paper fill。模型或策略版本变化必须产生新的结果版本，不覆盖旧记录。

## 15. 可观测性和报告

### 每日运行报告

- 发现比赛数、成功映射数和人工复核队列；
- Prediction 成功/失败/降级数量；
- quote 新鲜度和采集延迟；
- 策略机会、拒绝原因和 Paper fills；
- 真实订单状态、Shadow Paper 偏差和对账异常；
- 待结算、已结算和异常市场；
- 数据缺失、断线、重试和任务延迟。

### 每周研究报告

- 两策略 PnL、ROI 和权益曲线；
- drawdown；
- Brier、Log Loss 和 calibration；
- Edge bucket、赛区、Patch 和流动性分段；
- 与 Elo 和 Market baselines 的比较；
- Paper 与理论报价偏差；
- 真实 fill 与 Shadow Paper 的价格、fee、延迟和成交率偏差；
- 是否触发继续、暂停或 Kill 条件。

## 16. 安全要求

- `.env`、API keys、wallet keys、数据库备份和 raw data 不提交 Git。
- 日志不得输出 secrets、签名或完整鉴权头。
- `paper` 模式不得加载真钱下单凭证，默认配置不得启用真实执行。
- 研究数据库与真实执行钱包隔离。
- 依赖锁定并进行基础供应链审查。
- 所有生产配置生成 hash 并随交易记录保存。
- 真实执行使用最小权限钱包、提款隔离、人工总开关和硬编码之外的配置化风险上限。
- Git 跟踪的文档、日志样例、可公开报告和测试 fixture 不记录真实本金、钱包地址、账户标识、所在地或网络规避信息；必要的余额只保存在本地私有账本中。

## 17. 里程碑总览

| 里程碑 | 结果 | 预计时间 | 是否需要 VPS | 是否需要真钱 |
|---|---|---:|---:|---:|
| M0 最小骨架 | Rust、配置、日志、SQLite 可运行 | 2–4天 | 否 | 否 |
| M1 数据可行性 | 数据源、映射、历史报价等级明确 | 1–2周 | 否 | 否 |
| M2 历史数据集 | 500场以上可复现、无泄漏数据集 | 1–2周 | 否 | 否 |
| M3 预测研究 | 模型与基准完成 OOS 比较 | 2–4周 | 否 | 否 |
| M4 双策略历史研究 | 锁定策略配置并完成压力测试 | 1–2周 | 否 | 否 |
| M5 本地执行验证 | Paper、真实 smoke test、小资金双策略与对账跑通 | 2–3周开发后分级验证 | 否 | 条件性需要 |
| M6 可选持续运行 | 本地稳定后迁移长期 Paper/Shadow 或小资金实盘 | 按样本需求 | 2C2G | 取决于执行模式 |
| M7 条件性增强 | GPT 结构化信息 Shadow 实验 | M6后按证据 | 否 | 否 |

开发时间和采样时间必须分开看。代码写完不代表策略得到验证；最长时间来自等待真实比赛和不可回看的实时样本。

## 18. 当前优先级队列

### P0：立即执行

1. `PS-001`：补全 `.gitignore`。
2. `PS-002`：初始化简单 Rust 单程序，不提前搭建企业级分层。
3. `PS-003`：比较并记录第一批开源依赖。

具体任务 ID、依赖关系、交付物和验收条件统一维护在 [TASK_BREAKDOWN.md](./TASK_BREAKDOWN.md)。本节只保留优先级摘要，避免两处重复维护完整任务。

### P1：Gate 0 通过后

1. M2 完整历史 ETL 和无泄漏数据集。
2. M3 Baseline、预测模型和 calibration。
3. M4 双策略回放、depth-aware Paper fill 和独立账本。

### P2：Gate 2 通过后

1. M5 本地实时调度与 `T-15m` 快照。
2. 自动结算、日报和重启恢复。
3. 本地连续 Paper 和逐笔复核。
4. Gate 3-A 通过后完成真实下单 smoke test。
5. `live_smoke` 验收通过后在本地运行双策略小资金盘前实验和 Shadow 对账，完成后再作 Gate 3-B 判断。

### P3：本地真实执行稳定后

1. 根据持续运行需求决定是否购买 VPS，不把 VPS 当作下单前置。
2. 继续累积 Paper、真实 fill 和执行偏差样本。
3. 只有证据显示非结构化信息是主要误差来源时，启动 GPT shadow enhancement。

## 19. 当前锁定与延后决策

### 已锁定

- 项目总名为 ProbScout。
- Rust 是实时服务主语言，Python 只用于研究训练。
- 当前只研究 LOL 系列赛胜负市场。
- 第一版决策时点为 `T-15m`。
- 第一版比较 Threshold Strategy 与 Edge Strategy。
- 第一版每策略500U虚拟资金、每笔最多10U。
- 第一版统一持有到结算。
- 第一版严格限于 Prematch；比赛开始后不再生成或执行交易意图。
- 本地先运行 Paper，随后按 Gate 依次解锁 `live_smoke` 和双策略 `live_small`。
- 小资金实盘的真实本金和单笔数值只保存在 Git 忽略的本地配置，不写入计划、日志或报告。
- 每个真实订单必须保留同输入 Shadow Paper 对照。
- 普通 Riot Developer API 不接入交易研究链路。
- 本地验证通过前不购买 VPS 和昂贵实时数据。
- 当前按个人项目执行，不引入企业级工程流程。
- 通用能力必须 Open Source First；存在成熟适配库或 SDK 时禁止自行造轮子。
- 稳定计划和动态任务状态分离；普通 Task 不重复修改项目总计划。

### 延后到证据出现后决定

- 最终模型类型和具体特征集合。
- Threshold、min probability、min edge 和 uncertainty buffer 的最终值。
- 是否需要 GPT 增强。
- 是否购买 GRID/PandaScore/Abios。
- 是否以及何时迁移到 VPS。
- 小资金实验是否扩大运行时间或风险预算。

## 20. 完成定义

ProbScout 第一阶段不是以“机器人成功发出一次订单”为完成，而是以下结论之一能够被证据支持：

### Success

至少一个策略在时间外推、实时 Paper 和受控小资金实盘中，扣除真实执行成本后表现出可复核、跨时间相对稳定的正优势；真实执行与 Shadow Paper 的偏差处于风险预算内。

### Conditional Success

预测模型具有信息价值，且自动化链路基本可靠，但历史可成交数据、实时样本或真实执行样本不足；继续采集数据，不扩大工程和资金投入。

### Kill

模型不能稳定优于基准，或所有可观察收益在加入执行成本、时间切分和防泄漏规则后消失。此时停止项目，不降低标准制造交易机会。

核心原则保持不变：

> 先证明数据可信，再证明概率有信息，最后证明真实可成交条件下存在 Alpha。
