# Decision Log

## 2026-08-12 — Research First 与 Prematch Match Winner

第一版只验证职业 LOL 盘前系列赛胜负市场。原因是先隔离概率模型和市场错价能力，避免盘中状态、退出执行和额外市场类型同时引入混杂变量。影响是盘中交易、一血、一龙、一塔和单局市场均不属于当前版本。

## 2026-08-12 — 双策略共享输入、独立账本

Threshold Strategy 与 Edge Strategy 读取同一 Prediction 和同一 Market Quote，但各自维护账本和风险暴露。原因是直接比较高概率买入与净 Edge 过滤的效果，不能让资金共享污染实验结论。

## 2026-08-12 — 市场价格必须使用可成交成本

策略使用 order book depth、VWAP 和当时 fee 计算 `effective_entry_price`，不能使用页面展示概率代替。原因是 midpoint、ask、spread、深度和 fee 会改变真实期望值。

## 2026-08-12 — 来源时间不折叠

Leaguepedia `Scheduled Start`、Gamma `Market End` 与 CLOB `Game Start` 分别保存。原因是真实样例中 Gamma endDate 比 CLOB gst 晚 6 小时；静默合并会破坏盘前门禁。影响是自动匹配只比较具有开赛语义的时间，冲突必须进入人工检查或拒绝。

## 2026-08-12 — Gate 0 Conditional Go

M1 只以 `Conditional Go` 进入后续 Grade C 信号研究。原因是 50 场映射中 29 个自动 `Matched` 无观察错误、全部市场有双方历史 `{t,p}`，且单个开放市场的完整 REST order book 可读取和离线重放；反面证据是 21 场仍需人工复核、历史数据全部为 Grade C，且尚无 WebSocket 或持续实时采集稳定性验证。影响是 M2/M3 可继续，但 unresolved mapping 必须排除或解决，历史结果不得声称可成交 PnL，任何 execution-sensitive 结论前必须补足多市场持续 order book、fee、时间戳、重连和离线重放证据。

## 2026-08-12 — 三层数据目录与 Dataset Manifest v1

本地研究数据固定分为不可变 `data/raw/`、可重建 `data/processed/` 和派生 `artifacts/`。每个 processed dataset 必须用 Manifest v1 记录 raw 文件 SHA-256/采集时间、UTC 生成时间、Git commit 与 dirty diff hash、生成入口、输出 hash、row count 和 Event 时间范围。原因是后续队伍身份、结果、特征和数据划分必须能够回到实际输入字节与代码版本；影响是缺失 lineage、路径越界、hash/时间无效或零行的输出都不是有效 dataset。

## 2026-08-12 — 时间化显式身份解析

队伍和赛事品牌身份采用带来源与半开有效区间的显式映射。source ID 存在时只按 ID 解析，未知 ID 不回退名称；没有 ID 时才使用已登记的 source/name/time。原因是改名、名称复用、二队关系和来源修订会让无时间 alias 或 fuzzy match 静默合并不同实体；影响是 `Missing`/`Ambiguous` 必须人工处理或排除，且赛事品牌、赛季阶段与单场 Event 保持分离。

## 2026-08-12 — Series Result 与 Market Resolution 双证据

系列赛 label 只在 Leaguepedia `MatchSchedule` 最终比分/胜者与 Gamma 已关闭、resolved、唯一 0/1 outcome 结算指向同一 Canonical Team 时成立。原因是身份 mapping 只证明“这是同一场/同一队”，不证明结果或市场 outcome label 正确；影响是任一来源未完成、字段缺失或胜者冲突都 fail closed。重复 `series_id` 仅在核心事实完全相同才按证据键字典序合并，否则整组拒绝。

## 2026-08-12 — T-15m Feature Snapshot 与来源内历史 identity

HIST-004 固定在 Scheduled Start 前 15 分钟生成 Feature Snapshot，历史 series 只有最后一局结束时间不晚于 cutoff 才可进入特征；目标类型不接收比分、winner 或 market resolution。当前 HIST-002 证据不能把 2026-08 审核时点的 Canonical Team identity 外推 180 天，因此历史 form 只按 Leaguepedia 精确 source key 统计，名称变化默认损失召回而不猜测合并。影响是每个特征必须保存最新来源时间，未来补 rename 历史时必须先增加 time-bounded identity evidence。

## 2026-08-12 — Manifest v1 记录上游 processed dataset

processed dataset 消费另一个 processed dataset 时，使用可选 `upstream_datasets` 同时固定上游 manifest 与 output 的路径和 SHA-256；原始 API 响应仍进入 `raw_inputs`。原因是 HIST-004 直接消费 HIST-003，若只记录底层 raw 或生成参数会丢失实际中间数据版本。该字段对旧 manifest 默认空列表，保持 v1 向后兼容。

## 2026-08-12 — 连续时间划分与 Final Test Seal

HIST-005 使用按 Scheduled Start 的连续半开 UTC 窗口，顺序固定为 train、validation、calibration、final test；同一 series 唯一命中一个集合，禁止随机打散、窗口间隙和重叠。调参 manifest 不包含 final test IDs，只保存 count、window 和对规范排序成员的 SHA-256 commitment；release 必须提供冻结的 model artifact、model config 和 evaluation code hash，并重新核对 source membership。原因是仅用布尔标记无法防止标准训练流程意外读取 final test；影响是 M3 的开发入口只能读取前三组，MODEL-007 才能走显式 release。seal 是工作流门禁，不是对原始数据读取者的加密保密。

## 2026-08-12 — Processed-only lineage 允许空 raw_inputs

Dataset Manifest v1 在 `upstream_datasets` 非空时允许 `raw_inputs=[]`，但两者不能同时为空。原因是 HIST-005 只消费 HIST-004 processed output，没有新的直接 raw source；复制上游 raw list 会把间接依赖错误声明为直接依赖。旧 manifest 的 raw-only 路径保持兼容。

## 2026-08-12 — M2 数据质量 Gate 为 NotReadyForM3

HIST-006 的构建任务已完成，但数据就绪 Gate 明确为 `NotReadyForM3`。原因是当前只有 23/500 eligible series，覆盖 4 个 UTC 日期、1 个年份和单一 Patch `26.15`；虽然必填字段、跨数据集成员、时间防泄漏和 split commitment 均通过，DATA-009 的 50 个市场仍全部为 Grade C。影响是 MODEL-001 继续被阻塞，下一步必须扩展多时间段、多 Patch 的不可变历史语料并重跑 HIST-002–HIST-006；不得把任务勾选、低缺失率或 pipeline 测试通过解释为模型证据。

## 2026-08-13 — Series Result 与 Market Resolution Link 分离

HIST-007 将纯赛事 `Series Result` 与按 `(series_id, market_id)` 建立的可选 `Market Resolution Link` 分离。本决策 supersede 2026-08-12 “Series Result 与 Market Resolution 双证据”中“缺少 market resolution 即淘汰赛事结果”的部分，但保留 linked 子集的双证据一致性要求。原因是赛事最终比分/胜者可由 Leaguepedia `MatchSchedule` 与 `ScoreboardGames` 独立证明，强制要求 Polymarket 会把训练语料错误限制为有市场赛事。影响是 marketless BO3/BO5 可进入 Constant/Elo/统计模型，Market Baseline、Edge Strategy 和 PnL 仍必须要求已校验 link；存在 link 时 closed/resolved、outcome 顺序、唯一 0/1 winner 和 series winner 任一冲突都 fail closed。

## 2026-08-13 — Source-identity candidate 先于 Canonical identity 扩展

HIST-008 先建立只含 Leaguepedia source key 的 `Historical Series Candidate`，再由后续任务进行时间化 Canonical identity resolution。原因是批量把 `Team1`/`Team2` 或 `OverviewPage` slug 化会绕过 HIST-002，并错误合并改名、缩写、名称复用、Academy/Challengers/二队。MatchSchedule 与 ScoreboardGames 分开分页采集，确保缺少 games 的 series 进入 rejection 而非被 inner join 静默过滤。影响是 2,061 条结构候选不能直接计作 eligible series；必须先对 468 个 team source key 和 146 个 competition source key 输出 `Resolved`/`Missing`/`Ambiguous` 证据。

## 2026-08-13 — Cargo exact relation 与事件时点共同构成历史 Identity Evidence

HIST-010 只用 `TeamRedirects.AllName -> canonical page` 和 `Tournaments.OverviewPage -> League/Region` 的 exact Cargo relation，并与 HIST-008 MatchSchedule 的具体赛事时点组合成 `[start,start+1s)` identity period。Canonical ID 对 exact source identity 做 SHA-256，不从名称生成 slug；缺失 relation 不用 source key fallback，一对多 relation 保留全部候选为 Ambiguous。原因是当前 alias 表可以明确证明来源关系，但不能授权无证据的时间插值。影响是 1,778 candidates resolved，283 继续阻塞；当前 Cargo 页面未来修订时必须产生新 raw hash/version，不能覆盖本次证据。

## 2026-08-13 — M2 Gate 更新为 ReadyForM3

本决策 supersede 2026-08-12 “M2 数据质量 Gate 为 NotReadyForM3”。HIST-010 重建得到 1,778/500 eligible series、170 个 UTC 日期、13 Patch、6 Region，3,556 个 team-side feature source time leakage 为 0，因此按预注册的 volume 硬门槛更新为 `ReadyForM3`。影响是允许开始 Constant/Elo/统计概率模型；但单一年份、41.09% same-Patch unavailable、98 条 unresolved identity queue 和 50/50 Grade C market evidence 仍是 finding，Market Baseline、Edge、PnL 与 execution readiness 不随本 Gate 自动放行。

## 2026-08-13 — Constant Baseline 只拟合 train class prior

MODEL-001 使用 scikit-learn `DummyClassifier(strategy="prior")`，正类固定为 `team_1_win`，只从 train split 的 label 总体比例拟合一个无特征常数概率。validation/calibration 只评估同一概率，final test 在冻结前只保留 count、membership commitment 和 access policy。原因是 50% 虽简单但忽略当前标签方向基准率，而使用所有 development labels 会把 validation/calibration 信息泄漏进模型。影响是后续 Elo/统计模型统一与 train-prior Constant Baseline 比较；final-test release 仍需要 model artifact/config/evaluation code 三个 SHA-256，MODEL-001 不提前执行。
