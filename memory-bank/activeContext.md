# Active Context

更新日期：2026-08-12

## 当前状态

- 分支：`main`，直接向 `origin/main` 推送。
- M0 已完成；M1 已完成 `DATA-001` 至 `DATA-010`，Gate 0 结论为 `Conditional Go`。
- `DATA-008` 已固定并人工核验 50 场 recent historical Match Winner：5 分钟容忍值下 29 个 `Matched`、21 个 `NeedsReview`，自动 `Matched` 错误为 0。
- `DATA-009` 对同一 50 场的双方共 100 个 token 查询 T-15m 前 24 小时官方 `{t,p}` price history；A `0/50`、B `0/50`、C `50/50`、Unavailable `0/50`。
- `DATA-010` 允许 M2/M3 的 Grade C 信号研究；不授权历史可成交 PnL、实时盘口稳定性或真钱结论。DATA-005 只有单市场 REST snapshot，WebSocket、持续采集与断线恢复尚未实现或验证。
- 最近验证：DATA-005/008/009 离线复放、独立覆盖断言、文档链接/状态检查、`cargo fmt --check`、`cargo check --locked`、`cargo test --locked --lib`、`git diff --check` 全部通过；19 个 library tests 通过。

## 下一任务

下一任务是 `HIST-001`，定义 raw/processed/artifact 目录与可追溯 manifest；本轮未开始。只能在 `Conditional Go` 边界内建设 Grade C 信号研究数据链路，不得顺带实现策略、WebSocket 或交易代码。

关键验收边界：

- 队伍双方或 BO 类型硬矛盾进入 `Rejected`；开赛时间冲突进入 `NeedsReview`，均不得自动 `Matched`。
- Gamma `Market End` 不参与开赛时间一致性判断。
- Leaguepedia `Scheduled Start` 与 CLOB `Game Start` 分别保留；当前经 50 场核验保持 5 分钟容忍值，超过时进入 `NeedsReview`。
- outcome/token 必须保持 Polymarket index 顺序。
- 缩写、历史改名和二队关系不得由字符串相似度猜测，必须依赖显式 alias。
- DATA-008 核验表为 `docs/DATA_008_MAPPING_REVIEW.csv`；29/29 `Matched` 正确，21 个时间冲突差值为 10–90 分钟。
- DATA-009 覆盖表为 `docs/DATA_009_HISTORICAL_MARKET_GRADES.csv`；50/50 只有双方 `{t,p}` price history，决策时点 depth、bid/ask 和当时 fee 证据均为 0/50。
- Grade C 只允许信号研究，不得用于证明 spread、10U VWAP、slippage、fill failure 或历史可执行 PnL；Grade A 必须依赖赛前实时保存的不可变 order book 与 fee 证据。
- 自动下游输入只接受 `Matched`；21 个 `NeedsReview` 必须人工解决或排除。HIST-003 还必须核对赛事胜者与市场 resolution，不能把当前 identity mapping 当作 label 已验证。
- 要关闭实时稳定性条件，后续必须跨多个未来市场验证持续 order book 采集与离线重放；WebSocket 路径需覆盖订阅恢复、断线重连、全量重新同步、乱序和重复事件。

## 首次检查

进入新会话后先运行 `git status --short --branch` 和 `git log -3 --oneline`，确认远程提交与工作区状态，再读取 `docs/TASK_BREAKDOWN.md` 的 `HIST-001` 和 `docs/DATA_010_GATE_0_DECISION.md`；不得把 `Conditional Go` 简化成无条件 `Go`。
