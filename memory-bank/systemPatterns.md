# System Patterns

- Rust 单体服务，Tokio 异步运行；SQLx + SQLite 持久化和 migration。
- 资金、价格和 PnL 最终使用 decimal 类型，禁止用二进制浮点表示现金。
- 原始数据保存在 Git 忽略的 `data/`，仓库提交查询合同、hash、派生说明和可重放逻辑。
- 来源访问优先官方 API 或维护中的开源库；不逆向私有网页接口，不重复制造成熟组件。
- 数据、映射、quote、执行或账本状态不确定时 fail closed。
- `Market Quote` 指实际 bid/ask/depth/fee，不是页面 midpoint 或 last trade。
- `Scheduled Start`、Gamma `Market End`、CLOB `Game Start` 是不同语义，必须分别保存。
- 第一版只做 Prematch Match Winner，并采用 `HoldToResolution`；盘中退出是以后需要独立验证的 Exit Policy。
- 长期术语以根目录 `CONTEXT.md` 为准，任务状态以 `docs/TASK_BREAKDOWN.md` 为准。
