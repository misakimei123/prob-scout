# Active Context

更新日期：2026-08-12

## 当前状态

- 分支：`main`，直接向 `origin/main` 推送。
- M0 已完成；M1 已完成 `DATA-001` 至 `DATA-010`，Gate 0 结论为 `Conditional Go`；M2 的 `HIST-001`–`HIST-006` 已实现，但数据就绪 Gate 为 `NotReadyForM3`。
- `DATA-008` 已固定并人工核验 50 场 recent historical Match Winner：5 分钟容忍值下 29 个 `Matched`、21 个 `NeedsReview`，自动 `Matched` 错误为 0。
- `DATA-009` 对同一 50 场的双方共 100 个 token 查询 T-15m 前 24 小时官方 `{t,p}` price history；A `0/50`、B `0/50`、C `50/50`、Unavailable `0/50`。
- `DATA-010` 允许 M2/M3 的 Grade C 信号研究；不授权历史可成交 PnL、实时盘口稳定性或真钱结论。DATA-005 只有单市场 REST snapshot，WebSocket、持续采集与断线恢复尚未实现或验证。
- `HIST-001` 已建立 `data/raw/`、`data/processed/`、`artifacts/` 目录合同和 Dataset Manifest v1；processed output 必须回溯 raw hash、UTC 生成时间、Git commit/dirty diff、生成入口、output hash、row count 和 Event 时间范围。
- `HIST-002` 已建立带半开有效区间的 Canonical Team/Competition identity registry；12 组显式队名变体、21 组赛事 label mapping（17 个 canonical competitions）可回溯 DATA-008 review ID。本批 verified rename periods 为 0。
- `HIST-003` 已从固定 50 场审核基线生成 23 行 BO3/BO5 Series Result：29 个 `Matched` 中排除 6 个 BO1，21 个 `NeedsReview` 继续排除；21 场 BO3、2 场 BO5 的必填字段完整，Leaguepedia winner 与 Gamma resolution 23/23 一致。processed dataset SHA-256 为 `04ba36d93f8560d9d0ece628cc372ebcebac58f70e71e7674eb37fb25db9bf95`。
- `HIST-004` 已对 23 场目标生成固定 `T-15m` Feature Snapshot：180 天 Leaguepedia 查询 1,761 行，形成 855 个精确 source-key team observations，16 个不完整 series fail closed 排除；23/23 快照有历史来源，晚于 cutoff 的来源时间为 0。dataset SHA-256 为 `f13e74dd8c3b28d888075ad4fb6ac4616aa34c6c62049e7f4db323e31a76a2fb`。
- `HIST-005` 已按连续半开 UTC 日期窗口将 23 场划为 train 3、validation 7、calibration 6、final test 7。调参 manifest 只公开前三组 16 个 ID；final test 只保存 count、window 和 membership SHA-256 `c1965fff7cbeb0cece75b7a1e4429c1d9e7b65699b1aa8cf6cff44811137583e`，release 需要冻结 model artifact/config/evaluation code hash。
- `HIST-006` 已生成缺失率、时间/赛区/Patch、IQR 异常与 Grade C 边界报告。必填 Series/Feature 缺失 0，source-time leakage 0/46，Same-Patch unavailable 3/46，execution-grade market snapshot 缺失 50/50；报告 SHA-256 为 `eddd8534144ffdcd9a1ec0a15052395922a7c3675ede12dc768af1982f8a86a2`，同输入双构建一致。
- 最近验证：真实 HIST-003/HIST-004/HIST-005/HIST-006 manifest Rust 校验、6 个 data-quality 定向测试、`cargo fmt --check`、`cargo check --locked`、`cargo test --locked --lib`、四个 binary tests、报告双重放与 `git diff --check` 全部通过；60 个 library tests 通过。

## 下一任务

下一步不是 `MODEL-001`。当前仅 23/500 eligible series、4 个 UTC 日期、1 个年份和单一 Patch `26.15`，M2 Gate 明确为 `NotReadyForM3`。须先扩展多时间段、多 Patch 的不可变历史语料，重建 identity/result/feature/split/quality pipeline，并重新运行 HIST-006；不得降低 identity、时间防泄漏或 Grade C 边界来凑数量。

关键验收边界：

- 队伍双方或 BO 类型硬矛盾进入 `Rejected`；开赛时间冲突进入 `NeedsReview`，均不得自动 `Matched`。
- Gamma `Market End` 不参与开赛时间一致性判断。
- Leaguepedia `Scheduled Start` 与 CLOB `Game Start` 分别保留；当前经 50 场核验保持 5 分钟容忍值，超过时进入 `NeedsReview`。
- outcome/token 必须保持 Polymarket index 顺序。
- 缩写、历史改名和二队关系不得由字符串相似度猜测，必须依赖显式 alias。
- DATA-008 核验表为 `docs/DATA_008_MAPPING_REVIEW.csv`；29/29 `Matched` 正确，21 个时间冲突差值为 10–90 分钟。
- DATA-009 覆盖表为 `docs/DATA_009_HISTORICAL_MARKET_GRADES.csv`；50/50 只有双方 `{t,p}` price history，决策时点 depth、bid/ask 和当时 fee 证据均为 0/50。
- Grade C 只允许信号研究，不得用于证明 spread、10U VWAP、slippage、fill failure 或历史可执行 PnL；Grade A 必须依赖赛前实时保存的不可变 order book 与 fee 证据。
- 自动下游输入只接受 `Matched`；21 个 `NeedsReview` 必须人工解决或排除。HIST-003 已对 23 个 BO3/BO5 样本独立核对赛事胜者与市场 resolution，但这不自动解决剩余 mapping。
- Series Result 以 Leaguepedia `MatchId` 为键；重复候选只有核心事实完全一致才按证据键稳定合并，任一比分、winner、Patch、时间、队伍、competition 或 resolution 冲突都 fail closed。
- Feature Snapshot 固定 `T-15m`；目标合同不接收比分、winner 或 market resolution。历史结果只有最后一局结束时间不晚于 cutoff 才可使用，每个特征记录最新来源时间。
- HIST-004 历史 form 只按 Leaguepedia 精确 source key；不得把当前 Canonical Team identity 向历史外推。rename/alias 只有补齐带有效区间的 HIST-002 evidence 后才能合并。
- HIST-005 只按 Scheduled Start 使用 `[start,end)` 连续时间窗，不随机打散、不按小局拆分；同一 series 必须唯一命中一个集合。
- 调参只消费 train/validation/calibration IDs；final test 只发布 membership commitment。显式 release 前必须冻结 model artifact、model config 和 evaluation code 三个 SHA-256。
- HIST-006 的任务完成不等于 M2 Gate 通过；`NotReadyForM3` 时 MODEL-001 仍被阻塞。Same-Patch 无历史是显式 unavailable，不是 0% 胜率；IQR outlier 只进入 review，不自动删除。
- 要关闭实时稳定性条件，后续必须跨多个未来市场验证持续 order book 采集与离线重放；WebSocket 路径需覆盖订阅恢复、断线重连、全量重新同步、乱序和重复事件。
- processed dataset 必须位于 `data/processed/<dataset>/<version>/` 并有同目录 manifest；raw 输入不可原地修改，artifact 后续必须引用 dataset manifest 路径和 hash。
- source ID 存在时身份解析只按 ID；未知 ID 不回退名称。无 ID 时只按已登记的 source/name/observation time；`Missing`/`Ambiguous` 必须人工处理或排除。
- Academy、Challengers、二队默认独立；赛事品牌、season/stage 和单场 Event 不得折叠为同一概念。

## 首次检查

进入新会话后先运行 `git status --short --branch` 和 `git log -3 --oneline`，确认远程提交与工作区状态，再读取 `docs/TASK_BREAKDOWN.md` 的 `HIST-006`、`docs/PREMATCH_FEATURE_DATASET.md`、`docs/TEMPORAL_SPLIT_DATASET.md` 与 `docs/DATASET_LAYOUT.md`；质量报告不得把小样本或 Grade C 市场历史包装成 M2 Gate 已通过。
