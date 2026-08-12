# System Patterns

- Rust 单体服务，Tokio 异步运行；SQLx + SQLite 持久化和 migration。
- 资金、价格和 PnL 最终使用 decimal 类型，禁止用二进制浮点表示现金。
- 原始数据保存在 Git 忽略的 `data/`，仓库提交查询合同、hash、派生说明和可重放逻辑。
- 数据 lineage 固定为不可变 `data/raw/`、可重建 `data/processed/<dataset>/<version>/` 和 `artifacts/`；processed 文件必须带 Dataset Manifest v1。
- 队伍和赛事品牌使用 source-aware、time-bounded identity periods；source ID 优先且不按名称回退，缺失和歧义均 fail closed。
- Series Result 的赛事赛果与市场结算使用独立证据；只有 Canonical winner 一致才产出 label，重复键冲突 fail closed。
- 来源访问优先官方 API 或维护中的开源库；不逆向私有网页接口，不重复制造成熟组件。
- 数据、映射、quote、执行或账本状态不确定时 fail closed。
- `Market Quote` 指实际 bid/ask/depth/fee，不是页面 midpoint 或 last trade。
- `Scheduled Start`、Gamma `Market End`、CLOB `Game Start` 是不同语义，必须分别保存。
- 第一版只做 Prematch Match Winner，并采用 `HoldToResolution`；盘中退出是以后需要独立验证的 Exit Policy。
- 长期术语以根目录 `CONTEXT.md` 为准，任务状态以 `docs/TASK_BREAKDOWN.md` 为准。
- HIST-003 真实批次只消费 DATA-008 已解析 BO3/BO5，23/23 Leaguepedia winner 与 Gamma resolution 一致；HIST-004 必须与赛后结果字段隔离。
