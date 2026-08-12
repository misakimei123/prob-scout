# ProbScout 开源依赖清单

> 审核日期：2026-08-12
>
> 项目阶段：M0 / Research First
>
> 原则：这里只批准依赖方向。除已经写入 `Cargo.toml` 的库外，其他库在对应 Task 开始时再确认当前版本并加入锁文件。

## 1. 状态含义

| 状态 | 含义 |
|---|---|
| 已使用 | 已存在于当前项目和 `Cargo.lock` |
| 已选定 | 方向已确定，到对应 Task 才实际引入 |
| 条件使用 | 只有上层 SDK 未覆盖或实测出现需求时才引入 |
| 延后 | 当前没有足够需求，不加入依赖 |
| 禁止 | 已知不维护、恶意或违反项目边界，不得使用 |

## 2. Rust 运行程序

| 用途 | 选用库 | 状态 | License | 选用原因 | 主要备选与结论 |
|---|---|---|---|---|---|
| CLI | [`clap`](https://docs.rs/clap/latest/clap/) `4.6.6`，`derive` feature | 已使用 | MIT OR Apache-2.0 | 已提供标准 help、version、校验和未来 subcommand，避免手写参数解析 | `argh` 更轻但生态和能力较小；当前不切换 |
| Async runtime | [`tokio`](https://docs.rs/tokio/latest/tokio/) `1.53.1` | 已使用 | MIT | HTTP、WebSocket、SQLx 和 Polymarket 官方 SDK 都与 Tokio 生态兼容 | `async-std`、`smol` 会增加生态转换成本，不采用 |
| Polymarket API | 官方 [`rs-clob-client-v2`](https://github.com/Polymarket/rs-clob-client-v2)，crate `polymarket_client_sdk_v2` | 已选定 | MIT | 官方维护；提供 CLOB、Gamma、WebSocket、typed request/response 和 Decimal 等能力，禁止项目重写协议 DTO 与 endpoint | 原 `polymarket-client-sdk` 已归档且明确不可使用；第三方 SDK 不优先 |
| 通用 HTTP | [`reqwest`](https://docs.rs/reqwest/latest/reqwest/) | 已选定 | MIT OR Apache-2.0 | 成熟 async client，支持 JSON 与 rustls；也与官方 Polymarket SDK 的技术栈一致 | `hyper` 过于底层；`ureq` 是 blocking，不适合常驻 Tokio 服务 |
| WebSocket | Polymarket SDK 的 `ws` feature 优先；[`tokio-tungstenite`](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/) 作为其他来源备选 | 条件使用 | MIT | Polymarket 协议先复用官方 typed stream；只有其他数据源无 SDK 时才直接使用通用 WebSocket 库 | `fastwebsockets` 更偏性能；本项目不是 HFT，不为此增加复杂度 |
| Serialization | [`serde`](https://serde.rs/) + `serde_json` | Serde 已使用；JSON 已选定 | MIT OR Apache-2.0 | Rust 通用类型序列化标准生态，HTTP、CSV、配置和 SDK 广泛复用 | 手写 JSON/DTO 禁止；其他格式按真实需求再加 |
| Config | [`config`](https://docs.rs/config/latest/config/) | 已使用 | MIT OR Apache-2.0 | 支持默认值、文件和环境变量分层，符合个人项目的一份配置需求 | `figment`、`envy` 均可用，但不同时引入多个配置框架 |
| Structured logging | [`tracing`](https://docs.rs/tracing/latest/tracing/) + [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) | 已使用 | MIT | 适合 Tokio async task，可输出结构化事件并使用环境变量过滤 | `log` + `env_logger` 上下文能力较弱；不采用完整可观测平台 |
| 测试临时文件 | [`tempfile`](https://docs.rs/tempfile/latest/tempfile/) | 已使用（dev） | MIT OR Apache-2.0 | 安全创建并自动清理临时配置，避免自制临时文件名和清理逻辑 | `std::env::temp_dir` 需要自行处理冲突与清理，不采用 |
| SQLite | [`sqlx`](https://docs.rs/sqlx/latest/sqlx/) `0.9.0` 的 `sqlite`、`runtime-tokio`、`macros`、`migrate` 能力 | 已使用 | MIT OR Apache-2.0 | 提供 async SQLite、连接池和 embedded migration，满足单服务可恢复账本 | `rusqlite` 更轻但同步；本项目优先保持 Tokio 主链一致，不引入 ORM |
| 金额与价格 | [`rust_decimal`](https://docs.rs/rust_decimal/latest/rust_decimal/) | 已选定 | MIT | 固定精度，适合资金、价格、fee 和 PnL；避免二进制浮点误差 | `f32/f64` 禁止用于资金；`bigdecimal` 对当前精度需求过重 |
| CSV | [`csv`](https://docs.rs/csv/latest/csv/) + Serde | 已选定 | Unlicense OR MIT | 轻量、成熟，适合 Rust 导入 fixture 和导出个人报告 | `polars` 过重；复杂分析交给 Python |
| Date/time | [`chrono`](https://docs.rs/chrono/latest/chrono/) | 已选定 | MIT OR Apache-2.0 | Polymarket SDK 已重导出相关类型，可减少时间类型转换 | `time` 同样成熟，但同时使用两套时间类型没有收益 |
| Retry/backoff | 优先复用 SDK；否则在 API Task 中从 `tower` / `reqwest-retry` 选择 | 延后 | MIT | 不同请求的重试安全性不同，必须先定义幂等与预算，再启用成熟中间件 | 禁止手写通用 backoff；禁止对下单请求进行隐藏重试 |

## 3. Python 研究环境

Python 只用于数据处理、模型训练、校准和统计报告，不进入 Rust 常驻服务。当前不创建 Python 环境，也不固定尚未使用的版本。

| 用途 | 选用库 | 状态 | License | 选用原因 | 主要备选与结论 |
|---|---|---|---|---|---|
| 表格与 ETL | [`pandas`](https://pandas.pydata.org/docs/) | 已选定 | BSD-3-Clause | 对个人研究、时间序列、CSV 和数据质量检查最直接 | `polars` 性能更强，但当前数据规模不值得增加第二套 DataFrame API |
| Parquet | [`PyArrow`](https://arrow.apache.org/docs/python/parquet.html) | 已选定 | Apache-2.0 | 官方 Apache Arrow/Parquet 实现，可与 pandas 直接互操作 | Rust `parquet` crate、Polars 均延后，避免把分析依赖带入常驻服务 |
| 概率模型与校准 | [`scikit-learn`](https://scikit-learn.org/stable/modules/calibration.html) | 已选定 | BSD-3-Clause | 提供 Logistic Regression、概率校准、Brier、Log Loss 和标准评估能力 | XGBoost/LightGBM 仅在简单模型明确不足后评估；不自行实现优化器 |
| 数值计算 | [`NumPy`](https://numpy.org/doc/stable/) | 已选定 | BSD-3-Clause | pandas 和 scikit-learn 的基础依赖，也满足 Elo、bootstrap 和矩阵计算 | 不另建 Rust 数值训练栈 |
| 统计检验 | [`SciPy`](https://docs.scipy.org/doc/scipy/) | 条件使用 | BSD-3-Clause | 需要 bootstrap、分布或统计检验时直接复用 | 简单汇总优先使用 NumPy/scikit-learn，避免无需求引入 |
| 图表 | [`matplotlib`](https://matplotlib.org/stable/) | 条件使用 | PSF-based | 足以生成 calibration curve、权益曲线和 drawdown 静态图 | 不开发 Web Dashboard；seaborn 仅在确有统计图需求时再加 |

## 4. 明确禁止和延后

- 禁止使用已归档的 `Polymarket/rs-clob-client` 和旧 crate `polymarket-client-sdk`；官方仓库已声明其不再可用。
- 禁止安装 `clob-sdk`。该名称已被 [RustSec RUSTSEC-2026-0017](https://rustsec.org/advisories/RUSTSEC-2026-0017) 标记为恶意 typosquat。
- Polymarket 集成只从官方 `Polymarket/rs-clob-client-v2` 核对准确 crate 名和 feature。
- 当前不引入 `Diesel`、`SeaORM`、消息队列、任务编排框架、Web 框架或完整监控平台。
- 当前不引入 Rust ML 框架、XGBoost、LightGBM、深度学习框架和 LLM SDK。
- 当前不引入 Polars；只有 pandas/PyArrow 处理实际数据出现可复现瓶颈时再评估。
- 不直接启用 Polymarket SDK 的认证、下单、allowance、bridge、heartbeat 等真钱能力。Research/Paper 阶段只允许 read-only Gamma、CLOB market data 和必要的 `ws`。

本次候选均通过2026-08-12可访问的官方文档、官方仓库或官方包元数据核对维护状态和 License。若某库在实际接入时已归档、长期无维护或出现安全公告，当前“已选定”自动失效，必须重新比较候选；不得因为本文曾经选中过就继续使用。

## 5. 默认行为审计

每个库实际接入时必须检查并记录：

1. 只启用当前 Task 需要的 features，不使用无理由的 `full` 或 all-features。
2. HTTP timeout、connect timeout、redirect、proxy 和 TLS backend 必须显式设置。
3. 自动 retry 必须可见并计入请求预算；非幂等操作默认不重试。
4. SDK 的 heartbeat、自动取消订单、认证、遥测和数据外发功能默认关闭。
5. Tokio task、channel、连接池、分页和内存队列必须有上限。
6. SQLx 连接数、busy timeout、WAL 和 migration 行为必须显式配置并测试。
7. 日志不得包含 API Key、Authorization header、钱包私钥或完整原始 secret。
8. Python 模型必须记录包锁文件、随机种子、数据 manifest 和 artifact metadata。

## 6. 版本与供应链规则

- Rust 应用提交并维护 `Cargo.lock`；实际版本以锁文件为准。
- 尚未进入 `Cargo.toml` 的依赖不在本文固定版本，接入 Task 开始时重新检查官方文档和 MSRV。
- 安装前核对 crate 精确名称、owner、repository 和 license，尤其防止 Polymarket 相关 typosquat。
- 新依赖先使用最小 feature 集执行 `cargo check` 和相关窄范围测试。
- 后续建立依赖检查时优先采用 `cargo audit` 或 `cargo deny`，但当前不把重型供应链流程作为 M0 阻塞项。
- Python 环境建立后必须使用可提交的 lock 或带 hash 的依赖文件；不依赖全局 Python 环境。

## 7. 当前直接依赖

当前 `Cargo.toml` 的直接依赖为：

| Library | Version source | Features | 用途 |
|---|---|---|---|
| `clap` | `Cargo.lock` 当前解析为 `4.6.6` | `derive` | 最小 CLI、help 和 version |
| `config` | `Cargo.lock` 当前解析为 `0.15.25` | `toml`，关闭 default features | TOML 与环境变量分层配置 |
| `serde` | `Cargo.lock` 当前解析为 `1.0.229` | `derive` | 配置反序列化和后续领域类型序列化 |
| `sqlx` | `Cargo.lock` 当前解析为 `0.9.0` | `sqlite`、`runtime-tokio`、`macros`、`migrate`，关闭 default features | SQLite 连接池、查询和 embedded migration |
| `tokio` | `Cargo.lock` 当前解析为 `1.53.1` | `macros`、`rt-multi-thread` | async runtime 与 CLI 入口 |
| `tracing` | `Cargo.lock` 当前解析为 `0.1.44` | default | 结构化日志事件 |
| `tracing-subscriber` | `Cargo.lock` 当前解析为 `0.3.23` | `env-filter`、`fmt`、`json` | 文本/JSON 输出和 level 过滤 |
| `tempfile` | `Cargo.lock` 当前解析为 `3.27.0` | default，dev-only | 临时配置测试 |

其余依赖只在对应 Task 开始时按本清单逐项加入，禁止一次性把候选库全部安装。

## 8. 本次结论

- Rust 常驻服务路线：Tokio + 官方 Polymarket V2 SDK + reqwest + tracing + SQLx + rust_decimal。
- Python 研究路线：pandas + PyArrow + scikit-learn，按需增加 SciPy 和 matplotlib。
- Polymarket WebSocket 优先使用官方 SDK；`tokio-tungstenite` 只作未被 SDK 覆盖的数据源备选。
- 依赖清单服务于最小 Vertical Slice，不构建通用框架，也不提前安装未使用的库。
