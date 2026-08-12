# Active Context

更新日期：2026-08-12

## 当前状态

- 分支：`main`，直接向 `origin/main` 推送。
- M0 已完成；M1 已完成 `DATA-001` 至 `DATA-006`。
- `DATA-006` 已建立 `Event`、`TeamAlias`、`MarketMapping`、SQLite schema、可追溯解释和时间冲突输入。
- 最近验证：`cargo fmt --check`、`cargo check --locked`、`cargo test --locked --lib`、`git diff --check` 全部通过；10 个 library tests 通过。

## 下一任务

只执行 `DATA-007`：读取来源候选并生成 `Matched`、`NeedsReview`、`Rejected`。不要提前实现交易、模型或 DATA-008 人工核验。

关键验收边界：

- 队伍双方、BO 类型或开赛时间发生矛盾时不得自动 `Matched`。
- Gamma `Market End` 不参与开赛时间一致性判断。
- Leaguepedia `Scheduled Start` 与 CLOB `Game Start` 分别保留；超过容忍值进入人工检查或拒绝。
- outcome/token 必须保持 Polymarket index 顺序。
- 缩写、历史改名和二队关系不得由字符串相似度猜测，必须依赖显式 alias。

## 首次检查

进入新会话后先运行 `git status --short --branch` 和 `git log -3 --oneline`，确认远程提交与工作区状态，再读取 `docs/TASK_BREAKDOWN.md` 的 `DATA-007`。
