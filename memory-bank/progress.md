# Progress

更新日期：2026-08-13

## 已完成

- M0：`PS-001`–`PS-006`，Rust、配置、日志、SQLite migration 与质量命令骨架。
- M1：`DATA-001` 数据源登记。
- M1：`DATA-002` Oracle's Elixir 样本。
- M1：`DATA-003` Leaguepedia 样本。
- M1：`DATA-004` Polymarket Gamma LOL Match Winner catalog。
- M1：`DATA-005` CLOB 完整订单簿、fee 与含费 10U 理论 fill。
- M1：`DATA-006` 统一 Event、TeamAlias、MarketMapping 与来源时间语义。
- M1：`DATA-007` 自动候选匹配与 `Matched`、`NeedsReview`、`Rejected` 状态。
- M1：`DATA-008` 50 场人工映射核验；29 个自动 `Matched` 无错误，21 个时间冲突正确升级。
- M1：`DATA-009` 历史市场数据等级调查；固定 50 场 A 0、B 0、C 50，100 个 outcome token 的官方 `{t,p}` history 可离线重放。
- M1：`DATA-010` Gate 0 决策为 `Conditional Go`；允许 Grade C 信号研究，持续实时盘口和 WebSocket 稳定性未验证。
- M2：`HIST-001` 三层本地数据目录和 Dataset Manifest v1；processed dataset 可追溯 raw hash、生成时间与代码版本。
- M2：`HIST-002` 时间化 Canonical Team/Competition identity registry；12 组显式队名变体和 21 组赛事映射可回溯，未知/歧义 fail closed。
- M2：`HIST-003` 23 行 BO3/BO5 Series Result；Leaguepedia 最终赛果与 Gamma market resolution 独立核对，确定性去重与 Manifest v1 可复现。
- M2：`HIST-004` 23 行固定 `T-15m` Feature Snapshot；基础 team form 特征携带最新来源时间，目标赛后字段被拒绝，历史只按 Leaguepedia 精确 source key 且 cutoff 后记录不可见。
- M2：`HIST-005` 连续半开 UTC 时间窗口；23 场按 3/7/6/7 划分，final test 在调参阶段只发布 7 行 membership commitment，显式 release 需冻结模型与评估输入。
- M2：`HIST-006` 可重复数据质量报告；23/500、4 个 UTC 日期、单一 Patch，M2 Gate 为 `NotReadyForM3`。必填字段和时间泄漏为 0，Same-Patch unavailable 3/46，市场执行级证据缺失 50/50。
- M2：`HIST-007` 纯 `Series Result` 与可选 `Market Resolution Link` 解耦；marketless/linked 双模式得到相同纯结果，市场 link 独立 lineage 和 fail-closed 校验完成，下游 HIST-004/005/006 兼容重放通过。
- M2：`HIST-008` 2025 上半年 Leaguepedia 历史候选 corpus；9,935 个 MatchId 审计为 2,061 个 ready-for-identity candidate 与 7,874 个 rejection，覆盖 170 个 UTC 日期和 13 个 Patch source key，分页 raw 与 manifest 可重放。
- M2：`HIST-009` 时间化 identity coverage audit；2,061 candidates 全量审计，现有 2026 evidence 对 2025H1 得到 fully resolved 0、blocked 2,061，6,183 个 Missing occurrence 聚合为 614 条补证队列，输出与 manifest 可重放。
- M2：`HIST-010` exact Cargo identity evidence 与 Gate 复审；370/468 team keys、146/146 competition keys resolved，生成 1,778 Series/Feature rows、325/349/748/356 split，M2 Gate 更新为 `ReadyForM3`。
- M3：`MODEL-001` 训练期总体先验 Constant Baseline；325 条 train 得到 `P(team_1_win)=0.5230769231`，validation/calibration Brier 与 Log Loss 可重复计算，356 条 final test 保持 sealed。

## 待完成

- M3：下一任务仅执行 `MODEL-002` Elo Baseline；保持 final test sealed，不提前进入 MODEL-003、统计模型、策略或执行开发。

详细任务、依赖和验收条件见 `docs/TASK_BREAKDOWN.md`。
