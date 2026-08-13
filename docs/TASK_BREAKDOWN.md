# ProbScout 完整任务分解

> 用途：个人开发执行清单
>
> 配套计划：[DEVELOPMENT_PLAN.md](./DEVELOPMENT_PLAN.md)
>
> 更新规则：开始任务时将 `[ ]` 改为 `[-]`，完成并通过验收后改为 `[x]`。同一时间只保持一个主要任务为 `[-]`。
>
> 文档职责：本文件同时承担个人项目的 Task 目录和动态进度，不再额外维护 Jira、Sprint 或 `PROGRESS.md`。

## 1. 执行原则

- 按直接依赖推进；同一里程碑内没有依赖关系的任务可以按需要调整顺序。
- 未通过当前里程碑 Gate，不启动下一阶段的大规模工作。
- 每个任务以可运行结果或可复核文档结束，不以“写了一部分代码”结束。
- 优先调用成熟开源库；项目只实现领域特有逻辑。
- 任务实际不再需要时直接标记 `Deferred`，不为了完成清单制造无效代码。
- 活动路线为 M0–M6，M7 默认 Deferred；当前只启动 M0，不提前并行模型或交易代码。
- 不因某次回测结果好看而跳过数据质量和实时 Paper。

每个 Task 的完成闭环：

1. 检查直接依赖、范围和禁止事项；
2. 先查官方 SDK 与成熟开源库；
3. 完成最小充分实现，不顺手建设平台能力；
4. 执行该 Task 指定的窄范围验收；
5. 把状态、证据位置和真实限制写回本文件。

完成任务时，在对应 Task 下新增一行 `证据：`，链接测试输出、报告、fixture 或实现文件；尚未执行的任务不预填空白证据项。

Task 标记为 Done 不代表里程碑 Gate 自动通过。Gate 只依据累计数据作出 `Go`、`Conditional Go` 或 `Kill` 判断。

## 2. 状态定义

| 标记 | 状态 | 含义 |
|---|---|---|
| `[ ]` | Todo | 尚未开始 |
| `[-]` | In Progress | 当前正在执行 |
| `[x]` | Done | 已满足验收条件 |
| `[B]` | Blocked | 受外部数据、权限或环境阻塞 |
| `[D]` | Deferred | 当前阶段明确不做 |

## 3. 全局路线与依赖

```mermaid
flowchart LR
    M0["M0 最小骨架"] --> M1["M1 数据可行性"]
    M1 --> M2["M2 历史数据集"]
    M2 --> M3["M3 概率模型"]
    M3 --> M4["M4 双策略回测"]
    M4 --> M5A["M5-A 本地实时 Paper"]
    M5A --> M5B["M5-B 本地真实执行验证"]
    M5B --> M6["M6 可选 VPS 持续运行"]
    M6 --> GPT["M7 GPT Shadow"]
```

| 里程碑 | 活动 Task | 通过后得到什么 |
|---|---|---|
| M0 | `PS-001`–`PS-006` | 可运行的最小 Rust 研究骨架 |
| M1 | `DATA-001`–`DATA-010` | Gate 0：数据是否值得继续 |
| M2 | `HIST-001`–`HIST-010` | 可复现且无未来泄漏的数据集 |
| M3 | `MODEL-001`–`MODEL-007` | Gate 1：概率模型是否有信息价值 |
| M4 | `BACK-001`–`BACK-009` | Gate 2：双策略历史证据 |
| M5 | `LIVE-001`–`LIVE-009`、`REAL-001`–`REAL-003` | Gate 3：本地 Paper、真实 smoke 和双策略小资金链路是否可靠 |
| M6 | `OPS-001`–`OPS-005` | Gate 4：是否需要并能够安全持续运行 |
| M7 | `ENH-001` | 仅按长期证据启动的 GPT Shadow 实验 |

## 4. M0：仓库与最小运行骨架

目标：得到一个能编译、能运行测试、能读取配置、能写 SQLite 的最小 Rust 程序。不要在此阶段实现任何策略。

### [x] PS-001 补全本地文件忽略规则

- 依赖：无
- 修改：`.gitignore`
- 内容：`.env`、`data/`、SQLite 数据文件、Python cache、虚拟环境、模型 artifact 和本地日志。
- 验收：创建示例本地文件后 `git status --short` 不显示这些文件，源码和 migration 仍能正常显示。
- 证据：2026-08-12 使用6类实际样例验证全部被忽略，并确认 `Cargo.toml`、Rust 源码、migration、`.env.example` 和测试 fixture 均保持可跟踪；`git diff --check` 通过。

### [x] PS-002 初始化 Rust 项目

- 依赖：PS-001
- 输出：`Cargo.toml`、`src/main.rs`、`src/lib.rs`
- 要求：单 package、单 binary，不创建 workspace 和多个 service。
- 验收：`cargo check`、`cargo test` 通过；程序能输出版本和帮助信息。
- 证据：2026-08-12 `cargo fmt --check`、`cargo check` 和 `cargo test` 通过；1个单元测试通过；`cargo metadata` 确认为1个 package、1个 binary 和1个 library target；`--help`、`--version` 与默认启动输出验证通过。

### [x] PS-003 建立开源依赖清单

- 依赖：PS-002
- 输出：`docs/DEPENDENCIES.md`
- 至少评估：async runtime、HTTP、WebSocket、CLI、config、logging、SQLite、decimal、CSV/Parquet、统计/ML。
- 硬约束：存在满足边界的成熟开源库时必须采用；所有候选均不适配时，先记录证据、最小替代范围并获得用户确认，不得由实现者自行决定重造。
- 验收：每一行包含用途、库、license、选用原因和备选；不固定尚未实际使用的依赖版本。
- 证据：2026-08-12 创建 `docs/DEPENDENCIES.md`，覆盖11类必需能力和29行依赖记录；未使用依赖只记录方向、不固定版本；`cargo metadata --locked` 确认当前唯一直接依赖为 `clap 4.6.6` 且 License 为 MIT OR Apache-2.0；`cargo check --locked` 与 `cargo test --locked` 通过。

### [x] PS-004 接入最小配置和日志

- 依赖：PS-003
- 输出：配置加载和结构化日志。
- 要求：使用开源 config/CLI/logging 库；敏感值只来自环境变量，不打印 secrets。
- 验收：缺失必需配置时返回清晰错误；测试环境可使用临时配置；日志包含时间、level 和任务上下文。
- 证据：2026-08-12 使用 `config`、Serde、`tracing` 和 `tracing-subscriber` 完成 TOML + `PROB_SCOUT__*` 环境覆盖；3个配置测试覆盖临时文件、环境覆盖和缺失字段；默认与 JSON 启动日志均包含 timestamp、level 和 `task=startup`；缺失配置以退出码2失败；模拟 secret 未出现在日志；全仓4个测试通过。

### [x] PS-005 接入 SQLite 和 migration

- 依赖：PS-003
- 输出：数据库连接、第一份 migration、健康检查命令。
- 要求：使用成熟 Rust SQLite 库及其 migration 能力，不手写连接池和 migration runner。
- 验收：空数据库可以自动升级；重复运行 migration 不破坏数据；最小读写测试通过。
- 证据：2026-08-12 使用 SQLx 0.9.0 + Tokio 1.53.1 完成容量上限为5的 SQLite pool、WAL、5秒 busy timeout、foreign keys、embedded migration 和 `health` 子命令；临时数据库测试覆盖自动建目录/建库、migration、最小读写、关闭重开、数据保留和 migration 单次登记；`health` 对同一数据库连续运行2次成功；全仓5个测试通过。

### [x] PS-006 建立最小质量命令

- 依赖：PS-002
- 输出：README 中的本地验证命令。
- 命令：format check、`cargo check`、窄范围测试。
- 验收：本地一条简短命令或清晰的三条命令即可完成验证；不引入重型 lint 门禁。
- 证据：2026-08-12 在 `README.md` 记录 `cargo fmt --check`、`cargo check --locked`、`cargo test --locked --lib` 三条命令，并按相同顺序实际执行通过；library test 共5个；未增加脚本框架、lint 门禁或新依赖。

### M0 完成检查

- [x] Rust 程序可以在 Windows 本地编译运行。
- [x] 配置、日志和 SQLite smoke test 通过。
- [x] 通用能力均有开源库选择记录。
- [x] 尚未加入策略、交易和 VPS 代码。

M0 结论：`Go`。该结论只表示本地 Rust、配置、日志、SQLite 与最小质量检查可运行，不代表数据、模型或策略已经通过验证。

## 5. M1：数据可行性

目标：证明 LOL 比赛、Polymarket 市场和真实订单簿能够可靠获取并匹配。

### [x] DATA-001 建立轻量 Source Registry

- 依赖：M0 完成检查
- 输出：`docs/DATA_SOURCES.md`
- 数据源：Oracle's Elixir、Leaguepedia、GRID、Polymarket Gamma/CLOB。
- 验收：每个来源记录访问方式、用途、license/terms、研究/真钱限制和审核日期；明确普通 Riot Developer API 不接入。
- 证据：2026-08-12 创建 `docs/DATA_SOURCES.md`，登记 Oracle's Elixir、Leaguepedia、GRID/Riot Esports Data、Polymarket Gamma/CLOB 和排除的 Riot Developer API；逐项记录访问方式、数据用途、model training、Research/Paper、真钱、attribution、retention、redistribution、风险与审核日期，并提供13个权威/许可参考链接；依据当前官方页面将 GRID 免费接入假设纠正为“商业授权前阻塞”，将 Oracle's Elixir 整体 CSV 许可标为未明确；必填字段、链接格式和 `git diff --check` 验证通过。

### [x] DATA-002 下载 Oracle's Elixir 小样本

- 依赖：DATA-001
- 输出：可重复下载命令、raw 文件 hash、字段摘要。
- 范围：先取一个较小赛季或时间窗口，不下载全部历史。
- 验收：重复下载能识别相同文件；数据保存在 `data/raw/` 且不进入 Git。
- 证据：2026-08-12 新增 `research/download_oracles_elixir_sample.ps1` 和 `docs/ORACLES_ELIXIR_SAMPLE.md`；从 Oracle's Elixir 当前官方下载页取得 2025 年 CSV，只导出 2025-01-15 至 01-21 窗口；source SHA-256 为 `c9a158b9e0a965a47d31d3674c127a26f75e6c91a324bd1858e4784b1336214a`，sample SHA-256 为 `107c64b631df79208a53f34c6582349e402f5234d62e972d4644fdeba159f923`；样本包含 1,680 rows、140 games、165 columns，关键字段零空值；第二次运行返回 `cached/unchanged`，且 `git check-ignore` 确认所有 raw、sample、manifest 均被 `/data/` 排除。

### [x] DATA-003 获取 Leaguepedia 小样本

- 依赖：DATA-001
- 输出：赛程、队伍、赛事和 roster 示例。
- 要求：优先使用公开 API/Cargo 能力，不写 HTML scraper，除非确认无合适接口。
- 验收：至少能查询10场比赛及双方队伍标识，并保留来源时间戳。
- 证据：2026-08-12 新增 `research/download_leaguepedia_sample.ps1` 和 `docs/LEAGUEPEDIA_SAMPLE.md`；使用 Leaguepedia 官方 Cargo JSON export 单次查询 2025 Worlds 固定窗口，得到 10 rows、10 个唯一 `MatchId`、11 个唯一规范 team page，10/10 场双方 team page 与 tournament roster 非空；query SHA-256 为 `c439223469688f5fb7524fd45f51fb3b502d0e1a7a36f2b5e9a34e0bbbe31115`，raw response SHA-256 为 `4a13de1023f409081867ea8c9b70208330923500abce4589599eadda0f608be7`，来源采集时间为 `2026-08-12T06:15:19.3501349Z`；显式刷新确认响应 hash 不变，随后默认运行返回 `cached`；全过程未解析 HTML，raw/manifest 均由 `/data/` 排除。

### [x] DATA-004 获取 Polymarket 市场目录

- 依赖：DATA-001
- 输出：LOL event、market、condition ID、token ID 示例。
- 要求：优先使用官方 API 或维护中的开源 client；不逆向网页私有接口。
- 验收：能列出未来和历史 LOL Match Winner 候选，并保存原始响应 fixture。
- 证据：2026-08-12 新增 `research/download_polymarket_lol_catalog.ps1` 和 `docs/POLYMARKET_LOL_CATALOG_SAMPLE.md`；通过官方 Gamma `events/keyset`、LOL `tag_id=65` 获取以 `2026-08-12T06:00:00Z` 为边界的 future/historical 各 20 个 events，并用 `sportsMarketType=moneyline` 排除单局、totals、handicap 等非系列赛胜者市场；得到 future 20、historical 20 个候选，future 20/20 接受订单，40/40 具有 event/market/condition ID、两个 outcomes 和两个 CLOB token IDs；future/historical raw SHA-256 分别为 `a882a98628b63a1ed9887cc13de3322ae4e54bb0150c9b02640ab61ff3ea3b31`、`48a8667ceff5d16aaf4aa908f4a0dd2b46c2c834515954bb70f8fa256b207944`，派生 fixture SHA-256 为 `1517133042255ab7b1a953a2ff6ee5d376954a2bdd5dfa112bb2dc99bea04e6f`；`-Offline` 复跑返回双 scope `cached` 与 fixture `unchanged`，raw/fixture/manifest 均由 `/data/` 排除。

### [x] DATA-005 获取 Polymarket 订单簿

- 依赖：DATA-004
- 输出：best bid/ask、depth、tick size、minimum size、fee 信息。
- 验收：对一个开放市场计算10U理论 VWAP；明确 quote 接收时间；fixture 可离线重放。
- 证据：2026-08-12 新增 `research/capture_polymarket_order_book.ps1` 和 `docs/POLYMARKET_ORDER_BOOK_SAMPLE.md`；对 market `3422466` 的双方 token 批量获取完整 CLOB books，并通过 `clob-markets/{condition_id}` 保存 `gst`、token mapping、`tick_size=0.01`、`min_order_size=5` 和 fee schedule；quote 于 `2026-08-12T06:36:23.4929502Z` 接收，CLOB `gst=2026-08-12T08:00:00Z`，满足 15 分钟盘前门禁；DN SOOPers 与 Nongshim Red Force 的 best bid/ask 分别为 `0.38/0.39`、`0.61/0.62`，含 fee 的 10U effective entry price 分别为 `0.40189489`、`0.63178017`，均达到 95% fill 且满足 minimum size；market-info/books raw SHA-256 分别为 `083f537c982cd96f72ecc160acd0efe83a018cfdf30f1c0eae66935597c3192d`、`995f12a27fd279c69e457bb1a23ad3084c0ab14537ba99fd12994655f1563ff7`，派生 fixture SHA-256 为 `bce21066e484940417fb5a5a1a523b6a52bfec5328243d170b4fffe6aa8692a4`；连续 `-Offline` 复跑返回双 source `cached`、fixture `unchanged`。同时确认同一 event 的 Gamma `endDate` 与 CLOB `gst` 相差 6 小时，已将 CLOB `gst` 加赛事源交叉核验写为盘前硬门禁。

### [x] DATA-006 定义统一 Event 和别名

- 依赖：DATA-002、DATA-003、DATA-004
- 输出：最小 `Event`、`TeamAlias`、`MarketMapping` 数据结构。
- 要求：先用直接字段和少量规范化规则，不建立通用实体解析平台。
- 验收：能够解释一条映射使用了哪些来源 ID、队名和时间。
- 证据：2026-08-12 新增 `src/event_mapping.rs`、`migrations/202608120002_create_event_mapping.sql` 和 `docs/EVENT_MAPPING_SAMPLE.md`；使用 Leaguepedia `LCK/2026 Season/Rounds 3-4_Week 12_1`、Polymarket event/market `816302/3422466` 与 CLOB `gst` 建立可追溯样例。合同分别保存 Leaguepedia Scheduled Start `08:00Z`、Gamma Market End `14:00Z`、CLOB Game Start `08:00Z`，明确解释 6 小时时间语义差异；两个 outcome/token 按 index `0/1` 保序。名称规范化只处理大小写、空白和标点，缺少显式 alias、队伍不一致或 outcome 顺序错误时 fail closed；Rust 单元测试和 SQLite migration 测试覆盖映射说明、别名拒绝、时间不折叠与重开幂等。

### [x] DATA-007 实现候选自动匹配

- 依赖：DATA-006
- 输出：候选匹配和置信状态：`Matched`、`NeedsReview`、`Rejected`。
- 验收：时间、队伍双方和系列赛类型矛盾时不得自动匹配；不能确定时进入人工队列。
- 证据：2026-08-12 新增 `src/candidate_matching.rs` 和 `docs/CANDIDATE_MATCHING.md`；批量入口按 Gamma 输入顺序消费 DATA-006 `Event`、显式 `TeamAlias` 与有序 outcome/token，唯一且全部证据一致时生成 `Matched` + `MarketMapping`，队伍或 BO 硬矛盾生成 `Rejected`，缺 alias/开赛时间、超时差和多 Event 歧义生成 `NeedsReview`。时间仅比较 `Scheduled Start` 与 CLOB `Game Start`，Gamma `Market End` 不参与；8 个定向测试覆盖真实来源 ID、6 小时 Gamma offset、index 保序和全部状态，全库 `cargo fmt --check`、`cargo check --locked`、18 个 library tests 与 `git diff --check` 通过。

### [x] DATA-008 人工核验50场映射

- 依赖：DATA-007
- 输出：50场核验表和错误分类。
- 验收：自动 `Matched` 样本无错误；发现错误必须先修规则，再重新运行完整50场。
- 证据：2026-08-12 新增 `research/prepare_mapping_review.ps1`、`docs/DATA_008_MAPPING_REVIEW.csv` 和 `docs/DATA_008_MAPPING_REVIEW.md`；固定 Gamma 50 个 recent historical Match Winner、Leaguepedia 210-row 窗口和 50 个官方 CLOB metadata，逐场人工核对双方、BO、outcome/token index、`Scheduled Start` 与 `Game Start`。按 DATA-006 的 5 分钟容忍值得到 29 个 `Matched`、21 个 `NeedsReview`、0 个 `Rejected`；29/29 自动 `Matched` 人工确认正确，21/21 时间冲突均未误放行。完整 CSV SHA-256 为 `7fa2aa3d5ce52cf7f61041a2c94ef268120ee2828c3027f26531c2e1738d5d27`；离线复跑命中 50/50 CLOB cache，Rust 完整 50 场回放测试及全库 `cargo fmt --check`、`cargo check --locked`、19 个 library tests、`git diff --check` 通过。

### [x] DATA-009 调查历史市场数据等级

- 依赖：DATA-004、DATA-005
- 输出：Grade A/B/C 覆盖报告。
- 验收：报告明确多少场有 depth、bid/ask、只有 price history；不得把 Grade C 称为可成交回测。
- 证据：2026-08-12 新增 `research/audit_historical_market_data.ps1`、`docs/DATA_009_HISTORICAL_MARKET_GRADES.csv` 和 `docs/DATA_009_HISTORICAL_MARKET_COVERAGE.md`；以 DATA-008 固定 50 场、CLOB `game_start_time - 15 minutes` 为决策时点，对双方共 100 个 token 查询此前 24 小时、1 分钟 fidelity 的官方 `{t,p}` price history。覆盖结果为 Grade A `0/50`、Grade B `0/50`、Grade C `50/50`、Unavailable `0/50`；决策时点 depth、bid/ask 与当时 fee 证据均为 `0/50`，明确禁止用本结果声称可成交回测或历史可执行 PnL。在线保存 100 份 raw JSON 与 SHA-256 manifest，离线复跑命中 100/100 cache，覆盖 CSV SHA-256 为 `3a35d45259d057a485c7ddc668bf0411bb199a5cded2259f49fa87c2c4800414`；PowerShell 解析、50 行唯一性/无未来 point/等级断言、`cargo fmt --check`、`cargo check --locked`、19 个 library tests 与 `git diff --check` 通过。

### [x] DATA-010 作出 Gate 0 决策

- 依赖：DATA-008、DATA-009
- 输出：一页以内结论：Go、Conditional Go 或 Kill。
- 验收：结论引用实际覆盖率、映射错误和历史报价等级，不只写主观判断。
- 证据：2026-08-12 新增 `docs/DATA_010_GATE_0_DECISION.md`，结论为 `Conditional Go`。50 场人工核验中 29 个 `Matched`、21 个 `NeedsReview`、29/29 自动匹配未发现错误；12 个必查字段在 50 场中为 600/600 非空。历史报价 A `0/50`、B `0/50`、C `50/50`；DATA-005 只证明 1 个开放市场的完整 REST order book 可读取和离线重放，未证明 WebSocket、持续采集与断线恢复。结论允许 M2/M3 的 Grade C 信号研究，但禁止历史可成交 PnL 声称，并把 unresolved mapping、resolution 对齐、实时采集稳定性和用途边界列为继续条件。DATA-005/008/009 离线复放、独立覆盖断言、文档链接/状态检查、`cargo fmt --check`、`cargo check --locked`、19 个 library tests 与 `git diff --check` 通过。

### M1 Gate（Gate 0：数据可行性）

- [x] 至少50场映射完成复核。
- [ ] 实时订单簿可以稳定读取并离线重放：单市场 REST snapshot 与离线重放已通过，但持续采集、WebSocket 和断线恢复尚未验证。
- [x] 历史研究最高能达到的真实性等级已经明确。
- [x] 数据用途至少允许当前本地研究。

M1 结论：`Conditional Go`。允许进入 M2/M3 的 Grade C 信号研究；不授权历史可成交 PnL、实时稳定性或真钱结论。完整条件见 `docs/DATA_010_GATE_0_DECISION.md`。

## 6. M2：历史研究数据集

目标：生成不会使用未来信息、能够重复构建的赛前系列赛数据集。

### [x] HIST-001 定义 raw/processed/artifact 目录

- 依赖：M1 Gate（Gate 0）
- 输出：数据目录和 manifest 格式。
- 验收：每个 processed dataset 能追溯 raw 文件 hash、生成时间和代码版本。
- 证据：2026-08-12 新增 `research/initialize_dataset_layout.ps1`、`src/dataset_manifest.rs` 和 `docs/DATASET_LAYOUT.md`；幂等建立 Git 忽略的 `data/raw/`、`data/processed/`、`artifacts/` 三层目录，并定义 Dataset Manifest v1。每个 processed dataset 强制记录至少一个 raw source/path/SHA-256/captured time、UTC 生成时间、完整 Git commit、dirty diff SHA-256、生成入口与参数，以及 output path/hash/正 row count/Event 时间范围；未知版本、不安全或越界路径、重复 raw、时间倒置等均 fail closed。脚本连续运行两次均确认 3/3 目录存在且被 ignore；8 个 manifest 定向测试、`cargo fmt --check`、`cargo check --locked`、全库 27 个 library tests 与 `git diff --check` 通过。

### [x] HIST-002 统一队伍和赛事身份

- 依赖：HIST-001、DATA-006
- 输出：队伍别名、改名和赛事映射表。
- 验收：同一时期同一队伍不会因名称变体拆成多个实体；无法确认的记录不静默合并。
- 证据：2026-08-12 新增 `src/identity_registry.rs`、`migrations/202608120003_create_identity_registry.sql`、`docs/HIST_002_TEAM_ALIAS_REVIEW.csv`、`docs/HIST_002_COMPETITION_MAPPING.csv` 和 `docs/IDENTITY_REGISTRY.md`，并在 `CONTEXT.md` 固化 `Canonical Team`、`Team Identity Period`、`Canonical Competition` 与 `Competition Identity Period`。50/50 场赛事品牌证据汇总为 21 个跨来源 label mapping、17 个 canonical competitions；12 组基础规范化无法对齐的队名均为人工 `verified_explicit`。本批没有可审核改名生效区间，故 verified rename periods 明确为 0，未虚构改名。解析按 source ID 优先且不回退名称；缺失、区间外或多候选返回 `Missing`/`Ambiguous`。7 个身份定向测试覆盖时间化改名 fixture、真实 `LOS`/`LØS`、未知 source ID、名称复用歧义、赛事/Event 分离与 33 行审核表重放；SQLite 持久化测试、`cargo fmt --check`、`cargo check --locked`、全库 35 个 library tests 与 `git diff --check` 通过。

### [x] HIST-003 生成系列赛结果数据集

- 依赖：HIST-002
- 输出：每行一场 series 的赛前记录和最终结果。
- 验收：BO3/BO5、赛区、Patch、时间、双方和胜者字段完整；重复事件有确定性处理规则。
- 证据：2026-08-12 新增 `src/series_result.rs`、`research/build_series_result_dataset.ps1`、`src/bin/validate_dataset_manifest.rs` 与 `docs/SERIES_RESULT_DATASET.md`，并在 `CONTEXT.md` 固化 `Series Result`、`Result Evidence`、`Market Resolution Evidence`。固定 DATA-008 50 场中仅消费 29 个 `Matched`，排除 21 个 `NeedsReview` 和 6 个 BO1，生成 23 行（BO3 21、BO5 2），6 个赛区、Patch/时间/双方/比分/winner 必填缺失均为 0。Leaguepedia series winner 与 Gamma `closed + resolved + 0/1 outcomePrices` 的市场结算 23/23 一致；dataset SHA-256 为 `04ba36d93f8560d9d0ece628cc372ebcebac58f70e71e7674eb37fb25db9bf95`，Manifest v1 Rust 校验通过。重复 series 按证据键字典序稳定选主记录，核心事实冲突 fail closed；5 个定向测试覆盖 BO/比分、winner-resolution 冲突、顺序无关合并和冲突拒绝。`cargo fmt --check`、`cargo check --locked`、全库 40 个 library tests、validator binary test 与 `git diff --check` 通过。

### [x] HIST-004 生成赛前特征快照

- 依赖：HIST-003
- 输出：只使用比赛开始前数据计算的基础特征。
- 验收：每个特征有来源时间；测试能够证明赛后记录不会影响早期比赛特征。
- 证据：2026-08-12 新增 `src/prematch_features.rs`、`src/bin/build_prematch_feature_snapshots.rs`、`research/build_prematch_feature_dataset.ps1` 与 `docs/PREMATCH_FEATURE_DATASET.md`。目标合同类型层面排除比分、winner 和 market resolution；固定 `T-15m`，历史可用时间取最后一局 `DateTime_UTC + Gamelength_Number`，每个 count/ratio/rest 特征保存最新来源时间。5 个定向测试覆盖 cutoff、来源时间、缺失历史、重复/无效记录、拒绝目标赛后字段，并证明追加目标自身赛后结果不改变早期快照。真实 180 天构建读取 1,761 行、形成 855 个 team observations，16 个不完整 series fail closed 排除，生成 23/23 快照且来源时间违规为 0；dataset SHA-256 为 `f13e74dd8c3b28d888075ad4fb6ac4616aa34c6c62049e7f4db323e31a76a2fb`。历史身份只按 Leaguepedia 精确 source key，不外推 Canonical identity；Manifest v1 固定 HIST-003 上游 manifest/output hash。`cargo fmt --check`、`cargo check --locked`、全库 47 个 library tests、两个 binary tests、真实 manifest Rust 校验与 `git diff --check` 通过。

### [x] HIST-005 按时间划分数据

- 依赖：HIST-004
- 输出：train、validation、calibration、final test manifest。
- 验收：时间区间不重叠；同一系列赛不跨集合；final test 在调参期间不可使用。
- 证据：2026-08-12 新增 `src/temporal_split.rs`、`src/bin/build_temporal_split_manifest.rs`、`research/build_temporal_split_dataset.ps1` 与 `docs/TEMPORAL_SPLIT_DATASET.md`，并引入 RustCrypto `sha2 0.11.0`。四组使用连续半开 UTC 窗口，重复 ID、间隙、重叠、空集合和边界外 series 全部 fail closed；构建入口只读取 `series_id + scheduled_start_utc`。调参 manifest 只公开 train/validation/calibration IDs，final test 只保存 count、window 和 membership SHA-256，显式 release 需要冻结 model artifact/config/evaluation code 三个 hash 并重新核对 source commitment。真实 23 场按完整 UTC 日期划为 3/7/6/7，覆盖 23/23、无 series 重复或跨集合，final test JSON 无 `series_ids`；dataset SHA-256 为 `fefdb5ec783d12d73721f0fe05f71cc6ccfd6aefa56c588d372bc24c84f8cb1d`。6 个定向测试、全库 54 个 library tests、三个 binary tests、`cargo fmt --check`、`cargo check --locked`、真实 manifest Rust 校验、缓存重放和 `git diff --check` 通过。

### [x] HIST-006 输出数据质量报告

- 依赖：HIST-005
- 输出：缺失率、覆盖年份、赛区、Patch 和异常值摘要。
- 验收：每个缺失关键字段都有排除或降级规则；报告能重复生成。
- 证据：2026-08-12 新增 `src/data_quality.rs`、`src/bin/build_data_quality_report.rs`、`research/build_data_quality_report.ps1` 与 `docs/DATA_QUALITY_REPORT.md`。真实报告覆盖 HIST-003/004/005 的 23 个 series：必填 Series/Feature 缺失为 0，晚于 `T-15m` cutoff 的 source time 为 0，跨数据集成员和 final test commitment 重建一致；时间只覆盖 4 个 UTC 日期、1 个年份、单一 Patch `26.15`，6 个赛区。Same-Patch history 缺失 3/46（6.52%）按合同保留并降级，不伪造 0% 胜率；DATA-009 execution-grade snapshot 缺失 50/50，全部 Grade C。IQR 标记 same-Patch count 4 个、rest 2 个，只进入 review。报告版本 `2026-08-12.e678afb.hist006-v4` SHA-256 为 `eddd8534144ffdcd9a1ec0a15052395922a7c3675ede12dc768af1982f8a86a2`，相同输入双构建 hash 一致；M2 Gate 明确为 `NotReadyForM3`。

### [x] HIST-007 解耦 Series Result 与 Market Resolution Link

- 依赖：HIST-003、HIST-006 Gate 结论。
- 输出：不要求预测市场的纯 `Series Result` 合同，以及按 `(series_id, market_id)` 独立校验的可选 `Market Resolution Link`。
- 验收：无 market link 的完整 BO3/BO5 可进入结果数据集；存在 link 时仍须满足 closed/resolved、二元 outcome 顺序、唯一 0/1 winner 与 series winner 一致；市场证据有无不得改变纯结果字节；Market Baseline、策略和 PnL 只能消费 linked 子集。
- 证据：2026-08-13 更新 `src/series_result.rs`、`research/build_series_result_dataset.ps1`、HIST-006 兼容读取与相关合同文档。8 个 `series_result` 定向测试覆盖 marketless 构建、独立 link、winner 冲突、未知 series、稳定去重和冲突拒绝。固定 23 场分别以 `-SkipMarketResolution` 和 linked 模式真实重建：两条路径的纯结果 SHA-256 均为 `336f48a31f313bedce04b499865b7a7bd10657adf7774808cafae1a274ae5a8c`；linked 模式另生成 23 行、SHA-256 `cbc49d9ac8e5baedaf337b6ee618fd9d7bbc4df24a07ca73a2e3c1345a6e5946` 的 market link dataset，两份 manifest 均通过 Rust 校验。纯结果继续重放 HIST-004/005/006，特征、split 和质量报告 hash 与拆分前一致，Gate 仍为 `NotReadyForM3`。最终 `cargo fmt --check`、`cargo check --locked`、63 个 library tests、5 个 binary targets、PowerShell parser 与 `git diff --check` 全部通过。

### [x] HIST-008 构建多时间段、多 Patch 历史候选语料

- 依赖：HIST-007。
- 输出：分页、不可变的 Leaguepedia MatchSchedule/ScoreboardGames raw，以及 source-identity `Historical Series Candidate` 和逐项 rejection audit。
- 验收：至少 700 个 ready-for-identity BO3/BO5 候选、60 个 UTC 日期和 3 个 Patch source key；MatchSchedule 与 games 分开采集，缺 game 不得静默消失；所有候选保留 Scheduled Start、实际 completed time、双方/competition source key 和 result evidence；相同输入输出确定；Canonical identity 不得猜测。
- 证据：2026-08-13 新增 `src/historical_candidates.rs`、`src/bin/build_historical_candidate_audit.rs`、`research/build_historical_candidate_corpus.ps1` 与 `docs/HISTORICAL_CANDIDATE_CORPUS.md`。2025-01-01 至 2025-07-01 半开范围分页读取 20 个 MatchSchedule page（9,935 rows）和 28 个 ScoreboardGames page（13,987 rows），审计 9,935 个 MatchId，得到 2,061 个 ready-for-identity candidate、7,874 个显式 rejection，覆盖 170 个 UTC 日期、13 个 Patch source key、BO3 1,617/BO5 444。候选涉及 468 个 team source key、146 个 competition source key，未自动生成 Canonical ID。dataset SHA-256 为 `e80c7dcdff55b5f9c0b92e1669e6e95fdbb1a81a8c35bee339cad7ff7b43daa5`；相同输入双构建一致，48/48 raw hash、output hash 与 manifest 引用复核通过。9 个 HIST-008 定向测试、全库 72 个 library tests、6 个 binary targets、`cargo fmt --check`、`cargo check --locked`、PowerShell parser 与 `git diff --check` 全部通过。

### [x] HIST-009 审计历史候选的时间化 Identity Coverage

- 依赖：HIST-002、HIST-008。
- 输出：逐 series 的 team/competition `Resolved`、`Missing`、`Ambiguous` 结果，以及按 source key 聚合的人工补证队列。
- 验收：覆盖全部 candidates；只使用 observation time 有效的显式 evidence；Missing/Ambiguous 均 fail closed；不得用 fuzzy/slug 或当前名称倒推；相同输入输出确定并具备完整 lineage。
- 证据：2026-08-13 新增 `src/identity_coverage.rs`、`src/bin/build_identity_coverage_audit.rs`、`research/build_identity_coverage_audit.ps1` 与 `docs/IDENTITY_COVERAGE_AUDIT.md`。真实审计覆盖 HIST-008 全部 2,061 candidates：现有 HIST-002 evidence 仅在 2026-08 DATA-008 观测秒内有效，因此 2025H1 得到 fully resolved 0、blocked 2,061；team occurrence 为 Resolved 0 / Missing 4,122 / Ambiguous 0，competition 为 0 / 2,061 / 0。4,122 + 2,061 个缺口聚合为 614 条 review queue（468 team、146 competition）。dataset SHA-256 为 `a868952c4e6e1b0872d5786faa338d5c52dcefed724b15a9969252e263529b82`；相同输入双构建一致，upstream/output/3 份 raw snapshot hash 与 manifest 引用一致。5 个 HIST-009 定向测试、全仓 77 个 Rust tests、`cargo fmt --check`、`cargo check --locked`、PowerShell parser 与 `git diff --check` 全部通过。

### [x] HIST-010 补充 2025 时间化 Identity Evidence 并复审 M2 Gate

- 依赖：HIST-009。
- 输出：可回溯的 2025 team/competition identity periods、更新后的 coverage audit，以及重建的 HIST-003–HIST-006。
- 验收：只从明确来源证据建立 Canonical identity；每条 period 有有效区间和 evidence ref；不得根据 slug/fuzzy 自动确认；重建后重新核对 eligible 数量、时间/赛区/Patch 覆盖和 leakage，再独立判定 M2 Gate。
- 证据：2026-08-13 新增 `src/historical_identity.rs`、`src/bin/build_historical_identity_audit.rs`、`src/bin/write_historical_series_results.rs`、`research/build_historical_identity_evidence.ps1`、`research/build_expanded_series_result_dataset.ps1` 与 `docs/HISTORICAL_IDENTITY_EVIDENCE.md`。完整分页保存 5,779 条 TeamRedirects 和 10,421 条 Tournaments raw；只接受 `AllName -> canonical page` 与 `OverviewPage -> League/Region` exact relation，并与对应 MatchSchedule 时点组合成 1 秒 identity period。370/468 team keys、146/146 competition keys resolved，得到 1,778 fully resolved、283 blocked、98 条剩余 queue。重建 Series/Feature/Split/Quality 为 1,778 rows，覆盖 170 个 UTC 日期、13 Patch、6 Region，3,556 个 team-side source time leakage 为 0；split 为 325/349/748/356，final membership SHA-256 为 `c5b7295b8363bc62c4cbf8d1c0edc798179fa09ad6634060f5207b1397a39f1d`。Identity/Series/Feature/Split/Quality SHA-256 分别为 `e01d8a1fbcf547db23cff33b285a00a95cd663d42953fffde06069931a70fe50`、`9e7a1c2d23b13570f16329e733a13457c997826bbde9fcb6fa2ce0c00334ae99`、`3a29cbfc7a9311b6bf36837da0fc2c24df115175460251bab862c6de89d50ab3`、`1ff428ae74f1a4a7d32dc033244f0aa74ff6268a818303258a7a96c01d699258`、`9a32f02e0e1a348ce01a7603163b8ac55bb14bdfd59975f2d40852cd45b92342`。Identity、Series、Feature 和 Quality 重放一致，全部 manifest/raw/upstream/output hash 复核通过；全仓 84 个 Rust tests、`cargo fmt --check`、`cargo check --locked`、PowerShell parser 与 `git diff --check` 通过。M2 Gate 更新为 `ReadyForM3`。

### M2 完成检查（历史数据就绪）

- [x] 至少500场 eligible series：HIST-010 得到 1,778/500；其余 283 candidates 因 98 个 team source keys 未解析继续 fail closed。
- [x] 时间防泄漏测试通过：3,556 个 team-side feature source time 均不晚于 `T-15m`，追加赛后数据不会改变早期快照。
- [x] 数据集可由脚本从 raw/upstream manifest 重复生成：Identity、Series、Feature 与 Quality 同输入重放 hash 一致，五层 manifest lineage 复核通过。

M2 的 `HIST-001`–`HIST-010` 已闭合，Gate 判定更新为 `ReadyForM3`：1,778 条 eligible Series Result 超过预注册 500 条硬门槛，覆盖 170 个 UTC 日期、13 Patch、6 Region，且 3,556 个 team-side feature source time leakage 为 0。该结论只授权进入 Constant/Elo/统计模型开发；单一年份、same-Patch unavailable 41.09%、50/50 Grade C 市场证据和 98 条 unresolved identity queue 继续作为明确限制，不授权 execution/PnL 结论。

## 7. M3：概率模型

目标：先证明模型概率有信息价值，再谈策略收益。

### [x] MODEL-001 实现 Constant Baseline

- 依赖：M2 完成检查
- 输出：50%或训练期总体基准概率。
- 验收：final test 指标可计算。
- 证据：2026-08-13 新增 `pyproject.toml`、`uv.lock`、`research/model001_constant_baseline.py`、`research/build_constant_baseline.ps1`、`tests/test_model001_constant_baseline.py` 与 `docs/CONSTANT_BASELINE.md`。使用 scikit-learn `DummyClassifier(strategy="prior")` 仅从 325 条 train label 拟合 `P(team_1_win)=0.5230769231`；validation 349 条的 Brier/Log Loss 为 `0.2479537478/0.6890521453`，calibration 748 条为 `0.2474473942/0.6880387182`。356 条 final test 保持 sealed，artifact 不含 final IDs 或指标，但记录 Brier/Log Loss 计算合同及 release 所需三个冻结 hash。5 个 Python 定向测试覆盖训练隔离、final ID 拒绝、winner 校验和单类 slice 指标；真实 artifact 双构建 hash 一致，SHA-256 为 `39e55ce8f3f5e17bf69ba9c44c6eba994336e1738cc608aeb4431d49b940b3b2`。

### [x] MODEL-002 实现 Elo Baseline

- 依赖：MODEL-001
- 输出：按比赛时间顺序更新的 Elo 概率。
- 验收：某场比赛只能使用之前比赛更新的 rating；单元测试覆盖首次参赛和跨赛区场景。
- 证据：2026-08-13 新增 `research/model002_elo_baseline.py`、`research/build_elo_baseline.ps1`、`tests/test_model002_elo_baseline.py` 与 `docs/ELO_BASELINE.md`。全局 Elo 固定 initial/scale/K 为 `1500/400/20`，1,422 条 development Series Result 按 `(Scheduled Start, series_id)` 严格先预测后更新；首次参赛使用 1500，跨赛区沿用 Canonical Team rating，同队同一开赛时刻多场记录 fail closed。真实构建覆盖 319 个队伍，train/validation/calibration Brier 分别为 `0.2422573700/0.2427027093/0.2217084843`，Log Loss 分别为 `0.6775746090/0.6784341773/0.6348542326`；356 条 final test 保持 sealed。7 个定向测试覆盖首次参赛、pre-update prediction、当前赛果隔离、跨赛区、乱序、同起始冲突与 final ID 拒绝；artifact 双构建 SHA-256 一致，为 `49e71bdbc29b19f964cdd4f7db08f7f46d6b21eff981f566efd2541590255a40`。

### [x] MODEL-003 实现 Market Baseline

- 依赖：DATA-009、MODEL-001
- 输出：同一信息时点的市场概率基准。
- 验收：明确概率口径与交易 ask 口径不同；无可靠市场价格时不伪造基准。
- 证据：2026-08-13 新增 `research/model003_market_baseline.py`、`research/build_market_baseline.ps1`、`tests/test_model003_market_baseline.py` 与 `docs/MARKET_BASELINE.md`。只消费 DATA-008 人工确认 `Matched`、具有 Market Resolution Link 且 DATA-009 双方 price history 完整的公开 Development linked subset；双方分别取不晚于统一 CLOB `Game Start - 15m` cutoff 的最后一个 `p`，按显式 outcome 顺序映射到 `team_1_win`，不做归一化。实际纳入 train/validation/calibration `3/7/6` 场，Brier 为 `0.1544916667/0.0833964286/0.2553708333`，Log Loss 为 `0.4658236962/0.3087825440/0.7434727676`；兼容 split 的 7 场 final test 继续 sealed，当前 2025H1 模型语料的 356 场 final test 未读取。artifact 明确 `p` 不是 ask/bid/depth/fee 或可成交价格，并禁止 ROI/PnL 解释；7 个定向测试、19 个模型测试、84 个 Rust tests、格式/静态检查、三份 manifest 校验、双构建确定性和 `git diff --check` 通过。artifact SHA-256 为 `6dd7db70e085070d3e910e30f2ee105e6222b958f6a01cd2cca2348183432d9a`。

### [ ] MODEL-004 训练第一版统计模型

- 依赖：MODEL-002、HIST-005
- 输出：一个简单可解释模型。
- 要求：优先使用成熟开源 ML 库；不自行实现优化器和通用算法。
- 验收：训练流程固定随机种子和 artifact metadata；validation 结果可重复。

### [ ] MODEL-005 概率校准

- 依赖：MODEL-004
- 输出：raw probability、calibrated probability 和 calibration artifact。
- 要求：使用开源校准实现；不得在 final test 拟合。
- 验收：校准前后 Brier、Log Loss 和 calibration curve 可比较。

### [ ] MODEL-006 Walk-forward 评估

- 依赖：MODEL-005
- 输出：按时间窗口的模型与基准比较。
- 验收：报告包含整体、时间、赛区和赛制分段；不只展示最好窗口。

### [ ] MODEL-007 作出 Gate 1 决策

- 依赖：MODEL-006
- 输出：模型继续、回退或停止结论。
- 验收：final test 只运行一次主评估；后续模型修改必须产生新版本并保留旧结果。

### M3 Gate（Gate 1：概率模型）

- [ ] 模型不能在多个时间窗口稳定劣于 Elo Baseline。
- [ ] 概率校准没有明显系统性过度自信。
- [ ] 所有结果可由固定命令重复生成。

## 8. M4：历史双策略回测

目标：在证据允许的真实性等级下比较 Threshold Strategy 与 Edge Strategy。

### [ ] BACK-001 定义不可变策略配置

- 依赖：M3 Gate（Gate 1）
- 输出：Threshold、min probability、min edge、uncertainty buffer、stake 和退出规则配置。
- 验收：配置生成 hash；回测结果记录对应 hash。

### [ ] BACK-002 实现 Threshold Strategy

- 依赖：BACK-001
- 输出：纯函数式入场判断。
- 验收：概率刚好等于、略低和略高阈值的边界测试通过。

### [ ] BACK-003 实现 Edge Strategy

- 依赖：BACK-001
- 输出：使用 conservative probability 和 effective entry price 的判断。
- 验收：fee、slippage、uncertainty buffer 能正确降低 Edge；边界测试通过。

### [ ] BACK-004 实现10U Paper Fill

- 依赖：DATA-005、BACK-002、BACK-003
- 输出：遍历 ask depth 的 VWAP、shares、fee、cash debit 和拒绝原因。
- 要求：优先使用开源 decimal 库；只自行实现 ProbScout 的成交业务规则。
- 验收：多档、部分成交、无流动性、最小订单和 rounding fixtures 全部通过。

### [ ] BACK-005 实现双策略独立账本

- 依赖：PS-005、BACK-004
- 输出：两个500U Paper Account。
- 验收：同一比赛两个策略可以独立交易；任一策略结果不会修改另一策略余额。

### [ ] BACK-006 实现结算

- 依赖：BACK-005
- 输出：win、loss、void/cancel、异常待人工处理。
- 验收：重复执行不会重复入账；结算依据保存的市场规则和最终状态。

### [ ] BACK-007 实现指标报告

- 依赖：BACK-006
- 输出：PnL、ROI、Win Rate、Max Drawdown、Opportunity、Fill Rate 和 Edge buckets。
- 要求：优先使用开源统计和绘图库；先输出 CSV/Markdown，不开发 Web UI。
- 验收：指标有小型人工算例对照；报告同时显示亏损和拒绝交易。

### [ ] BACK-008 执行压力测试

- 依赖：BACK-007
- 输出：基准、额外 slippage、失败成交和 fee 场景。
- 验收：结果按 Grade A/B/C 标记；Grade C 不声称历史 PnL 可成交。

### [ ] BACK-009 作出 Gate 2 决策

- 依赖：BACK-008
- 输出：继续实时 Paper、继续收集数据或 Kill。
- 验收：结论明确哪个策略更优、置信度和主要反例；不因结果不好而临时修改阈值。

### M4 Gate（Gate 2：历史双策略）

- [ ] 双策略使用完全相同的 Prediction 和 Quote 时点。
- [ ] 账本、成交和指标人工算例一致。
- [ ] 历史结果证据等级表述准确。

## 9. M5：本地实时 Paper 与真实执行验证

目标：先在本地跑通真实时间 Paper 链路，再按准入 Gate 验证真实下单和双策略小资金盘前运行；本阶段不需要 VPS。

### [ ] LIVE-001 自动发现目标市场

- 依赖：M4 Gate（Gate 2）
- 输出：去重后的 LOL Series Winner 市场目录。
- 验收：只接收 LOL Series Winner；重复发现不会重复创建 event。

### [ ] LIVE-002 维护赛事时间和状态

- 依赖：LIVE-001
- 输出：带原始计划、最新计划和实际开赛时间的赛事状态。
- 验收：保存原始、最新和实际开赛时间；延期和取消不会触发错误交易。

### [ ] LIVE-003 执行 T-15m 快照

- 依赖：LIVE-002、MODEL-005
- 输出：不可变 Feature Snapshot 与 Prediction。
- 验收：每场只生成一个主 Prediction；服务重启不会重复生成。

### [ ] LIVE-004 捕获真实 Market Quote

- 依赖：LIVE-003、DATA-005
- 输出：带接收时间的真实 order book snapshot。
- 验收：保存完整 depth、fee 和时间；quote 过期时拒绝交易。

### [ ] LIVE-005 运行双策略 Paper 决策

- 依赖：LIVE-004、BACK-005
- 输出：两个策略各自的 TradeIntent、拒绝原因与 Paper fill。
- 验收：生成 TradeIntent、拒绝原因和 Paper fill；不调用真钱下单接口。

### [ ] LIVE-006 自动结算和恢复

- 依赖：LIVE-005、BACK-006
- 输出：幂等结算任务、恢复流程和人工异常队列。
- 验收：断线、重启和重复事件后账本仍一致；异常结算进入人工队列。

### [ ] LIVE-007 生成个人日报

- 依赖：LIVE-006
- 输出：一份 Markdown 或终端报告。
- 验收：显示比赛、预测、报价、策略决策、余额、待结算和错误；不开发 Dashboard。

### [ ] LIVE-008 人工复核前20–30笔

- 依赖：LIVE-007
- 输出：逐笔核对表。
- 验收：预测时点、ask/depth、fee、策略决策和结算全部可追溯；差异修复后重新核验。

### [ ] LIVE-009 本地连续运行 Paper

- 依赖：LIVE-008
- 输出：至少3–7天运行记录与完整性摘要。
- 验收：覆盖断线、重启和至少一个完整结算周期；关键任务成功率、重复交易数、缺失 quote、异常结算和恢复有明确统计。

### M5-A Gate（Gate 3-A：本地 Paper 稳定性）

- [ ] 链路在本地连续运行至少3–7天。
- [ ] 重复交易和重复结算为0。
- [ ] 首批机会和 Paper fills 人工复核通过。
- [ ] 默认配置只能进入 `paper`，无法意外加载真实凭证。

### [ ] REAL-001 完成真实执行准入评审

- 依赖：M5-A Gate（Gate 3-A）
- 输出：数据用途、账户资格、平台访问资格、wallet secret、订单权限、风险上限和人工总开关检查结果。
- 验收：所有门槛逐项通过；不得把 VPN、代理或更换服务器地区作为规避平台限制的实现方案；真实本金、钱包和账户信息不写入仓库或可公开报告。

### [ ] REAL-002 完成本地真实下单 smoke test

- 依赖：REAL-001
- 输出：`live_smoke` 下的单一极小额测试订单、成交/拒单回报、fee、结算和对账记录。
- 验收：鉴权、签名、价格保护、订单状态、结算和账本全部可复核；超时、重试和重启不产生重复订单；失败时自动回到 fail closed。

### [ ] REAL-003 运行本地双策略小资金实验

- 依赖：REAL-002
- 输出：两个策略的独立虚拟子账本、真实 fills、Shadow Paper 和执行偏差报告。
- 验收：只做 Prematch；比赛开始后不产生新订单；每个真实订单都有关联 Shadow Paper；固定小额单笔、策略暴露、全局暴露、累计亏损和人工总开关均由 Risk Manager 强制执行；具体资金数值仅保存在 Git 忽略的本地配置。

### M5-B Gate（Gate 3-B：本地真实执行）

- [ ] smoke test 的订单、fee、结算和账本全部对平。
- [ ] 双策略归因与真实/Shadow 执行偏差能够独立计算。
- [ ] 重启、断线和重复消息不会产生重复真实订单。
- [ ] 任一数据、资格、账本或风险异常都会阻止新开仓。
- [ ] 本地运行稳定前不购买 VPS，也不扩大实验资金风险。

## 10. M6：可选 VPS 持续运行

目标：只有本地 Paper、真实 smoke 和小资金双策略链路稳定，并且确实需要长期无人值守时才支付服务器成本。VPS 不是开始真实下单的前置条件。

### [ ] OPS-001 准备最小部署

- 依赖：M5-B Gate（Gate 3-B）
- 输出：2 vCPU、2 GB RAM、30–40 GB SSD 的 Ubuntu LTS 部署说明。
- 验收：单 systemd service 能启动、停止和自动恢复；不引入 Docker 编排或 Kubernetes，除非出现明确需求。

### [ ] OPS-002 数据备份和恢复

- 依赖：OPS-001
- 输出：备份命令、保留规则和恢复说明。
- 验收：SQLite、配置和模型 artifact 可以从备份恢复；至少完成一次恢复演练。

### [ ] OPS-003 最小告警

- 依赖：OPS-001
- 输出：面向个人维护者的最小异常通知。
- 验收：进程退出、磁盘不足、连续采集失败和待结算异常能够通知个人维护者；不搭建企业监控平台。

### [ ] OPS-004 累积长期 Paper、Shadow 与真实执行样本

- 依赖：OPS-002、OPS-003
- 输出：冻结口径的长期 Paper/Shadow/真实执行数据集与每周报告。
- 验收：按观察方差持续采样，记录停机和漏采窗口；不将漏采比赛补成虚构交易；不使用固定笔数自动宣布盈利。

### [ ] OPS-005 作出 Gate 4 决策

- 依赖：OPS-004
- 输出：继续 Paper/小资金实验、研究增强、维持或停止 VPS、或 Kill。
- 验收：使用净 ROI、置信区间、Max Drawdown、稳定性和执行偏差作决定。

### M6 Gate（Gate 4：长期持续运行）

- [ ] 样本足以估计策略方差和真实执行偏差，或按观察方差给出继续采样理由。
- [ ] 历史与实时表现差异已解释。
- [ ] 报告同时包含净 ROI、置信区间、Max Drawdown、校准、真实/Shadow 偏差和运行缺口。
- [ ] 已明确决定继续 Paper、启动某个 M7 子任务或 Kill。

## 11. M7：条件性 GPT 增强

以下任务默认 Deferred，只有前置 Gate 提供证据后才启动。

### [D] ENH-001 GPT Shadow Enhancement

- 前置：长期 Paper 模型稳定，且非结构化信息缺失被证明是主要误差来源。
- 输出：结构化事件抽取器、Shadow Prediction 和增量评估报告。
- 验收：GPT 只提取结构化事件；Shadow 模型 OOS 指标改善后才影响 Prediction。

盘间和盘中预测、止损及动态退出不属于当前任务路线；未来若重新提出，必须另建独立计划，不能直接复用 Prematch 模型和本任务清单。

## 12. Open Source First 检查表

每个涉及通用能力的任务完成前检查：

- [ ] 已查官方 SDK 或官方推荐实现。
- [ ] 已查 crates.io/PyPI/GitHub 的维护中项目。
- [ ] 已检查 license 和基本维护状态。
- [ ] 已检查默认 timeout、retry、遥测、分页和数据外发行为。
- [ ] 没有复制无 license 的代码。
- [ ] 没有手写 HTTP、WebSocket、SQLite、Decimal、CLI、日志、重试或常见 ML 算法。
- [ ] 新增依赖已写入 `docs/DEPENDENCIES.md`。
- [ ] 自研代码确实属于 ProbScout 领域逻辑；若没有合适库，已记录候选缺口并获得用户确认。
- [ ] 没有为了测试方便创建多层无业务价值的 wrapper。

## 13. 当前下一步

当前继续按以下顺序推进，不并行展开模型和交易代码：

1. `PS-001`：已完成 `.gitignore`。
2. `PS-002`：已完成最小 Rust 项目。
3. `PS-003`：已完成开源依赖清单。
4. `PS-004`：已完成最小配置和结构化日志。
5. `PS-005`：已完成 SQLite、migration 和健康检查。
6. `PS-006`：已完成最小质量命令，M0 Gate 为 `Go`。
7. `DATA-001`：已完成轻量 Source Registry；GRID 当前阻塞，普通 Riot API 明确排除。
8. `DATA-002`：已完成 Oracle's Elixir 小窗口、hash、字段摘要和重复缓存验证。
9. `DATA-003`：已完成 Leaguepedia Cargo 小样本、规范队伍标识、roster、hash 和重复 cache 验证。
10. `DATA-004`：已完成 Polymarket LOL future/historical 市场目录、ID、raw hash 和离线 fixture 验证。
11. `DATA-005`：已完成双方 token 完整订单簿、fee、含费 10U VWAP、quote 时间和离线重放验证。
12. `DATA-006`：已完成最小 Event、TeamAlias 和 MarketMapping 合同，并显式保留 Leaguepedia/Gamma/CLOB 时间语义。
13. `DATA-007`：已完成候选自动匹配和 `Matched`、`NeedsReview`、`Rejected` 状态。
14. `DATA-008`：已完成人工核验 50 场映射；29 个自动 `Matched` 无错误，21 个时间冲突正确进入 `NeedsReview`。
15. `DATA-009`：已完成历史市场数据 Grade A/B/C 调查；固定 50 场全部只有 `{t,p}` price history，均为 Grade C。
16. `DATA-010`：已完成 Gate 0 决策，结论为 `Conditional Go`；只允许 Grade C 信号研究，实时订单簿持续稳定性仍是未关闭条件。
17. `HIST-001`：已完成 raw/processed/artifact 目录与 Dataset Manifest v1，可从 processed output 回溯 raw hash、生成时间和代码版本。
18. `HIST-002`：已完成时间化队伍/赛事身份合同、SQLite schema 与 50 场显式映射审核；未观察到可审核的真实改名区间。
19. `HIST-003`：已生成 23 行可追溯 BO3/BO5 series result；21 个 `NeedsReview` 和 6 个 BO1 明确排除，23/23 winner/resolution 一致。
20. `HIST-004`：已生成固定 `T-15m` 的 23 行赛前特征快照；每个基础特征带最新来源时间，目标赛后字段被类型合同拒绝，晚于 cutoff 的历史记录不会改变快照。
21. `HIST-005`：已按连续 UTC 日期窗口固定 train/validation/calibration/final test 为 3/7/6/7；final test 在调参 manifest 中只发布 count 与 membership commitment。
22. `HIST-006`：已生成可重复的数据质量报告；M2 Gate 为 `NotReadyForM3`，原因是仅 23/500、4 个 UTC 日期和单一 Patch，且市场证据 50/50 为 Grade C。
23. `HIST-007`：已将纯 `Series Result` 与可选 `Market Resolution Link` 解耦；marketless 赛事可进入模型语料，linked 子集才允许进入 Market Baseline、策略与 PnL 研究。
24. `HIST-008`：已建立 2025 上半年 Leaguepedia 历史候选 corpus；2,061 个 ready-for-identity candidate 覆盖 170 个 UTC 日期和 13 个 Patch source key，7,874 个不合格 series 保留明确 rejection。
25. `HIST-009`：已对 2,061 candidates 执行 Scheduled Start 时点 identity coverage；现有 2026 evidence 对 2025H1 无 active period，0 条 fully resolved，614 条聚合补证队列完整保留。
26. `HIST-010`：以 exact TeamRedirects/Tournaments relation 补充事件时点 identity evidence，得到 1,778 eligible Series Result 并重建 Feature/Split/Quality；M2 Gate 更新为 `ReadyForM3`。
27. `MODEL-001`：已实现训练期总体先验 Constant Baseline；固定 `P(team_1_win)=0.5230769231`，development Brier/Log Loss 可重复计算，356 条 final test 继续 sealed。
28. `MODEL-002`：已实现全局 chronological Elo Baseline；1,422 条 development 逐场先预测后更新，首次参赛与跨赛区合同已测试，356 条 final test 继续 sealed。
29. `MODEL-003`：已实现统一 `Game Start - 15m` cutoff 的 Grade C Market Baseline；16 场公开 Development linked series 的概率信号可重复计算，`p` 与 ask/depth/fee 明确分离，兼容 split 的 7 场 final test 继续 sealed。

M0 已完成；M1 已以 `Conditional Go` 通过 Gate 0；M2 的 `HIST-001`–`HIST-010` 均已实现并记录证据，数据就绪 Gate 为 `ReadyForM3`；M3 已完成 `MODEL-001`–`MODEL-003`。下一任务是 `MODEL-004` 第一版统计模型；不得提前进入概率校准、Walk-forward、策略、PnL 或执行开发。
