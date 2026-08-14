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

## 2026-08-13 — Elo 使用全局 chronological rating pool

MODEL-002 固定 initial rating 1500、scale 400、K-factor 20，对 train/validation/calibration 的 Series Result 按 `(Scheduled Start, series_id)` 逐场先预测后更新。首次参赛使用初始 rating，跨赛区沿用同一 Canonical Team rating，不引入 region reset 或未经验证的赛区修正；同队同一开赛时刻出现多场时 fail closed。原因是 Elo baseline 必须只使用当前比赛前可得赛果，同时保留国际赛事对全局队伍强弱的直接比较。影响是 development prediction 可 chronological 更新，但 final test 仍需冻结后显式 release；当前参数是固定 baseline，不是 validation 调优结果。

## 2026-08-13 — Market Baseline 保留 Grade C 原始 p 并与 ask 分离

MODEL-003 只对人工确认 `Matched` 且具有 Market Resolution Link 的公开 Development series，在统一 CLOB `Game Start - 15m` cutoff 分别选择双方官方 price history 的最后一个 `p`，按显式 outcome 顺序映射到 `team_1_win`，不做归一化。缺失 point、未来 point、未确认映射或 outcome/resolution 冲突均 fail closed。原因是 DATA-009 没有历史 bid/ask、depth 或 fee，任何补成 ask 或可成交成本的处理都会伪造证据。影响是当前 16 场 linked sample 只支持 Brier/Log Loss 信号研究；不得计算历史 ROI/PnL，也不得与不同母体的 2025H1 Constant/Elo 指标直接比较。

## 2026-08-13 — 第一版统计模型使用 train-only form-difference LogisticRegression

MODEL-004 使用 scikit-learn `StandardScaler + LogisticRegression`，只在 train 上拟合双方 `T-15m` prior-series/game/same-Patch form 差值、历史量差、rest 差、availability 差与 BO5 标记。无历史胜率在模型矩阵中使用中性 0.5，同时以 availability 差值保留缺失语义；validation/calibration 不参与任何参数拟合，输出保持 raw uncalibrated。原因是该方案简单、可解释、可重放且不自行实现优化器，并避免把 unavailable 误作 0% 胜率。影响是 MODEL-005 可独立消费 calibration split 做校准；MODEL-004 指标不能替代 Walk-forward 或 Gate 1 结论。

## 2026-08-13 — 校准器只消费冻结 raw probability 与 calibration label

MODEL-005 使用 scikit-learn `CalibratedClassifierCV(method="sigmoid")`，以 `FrozenEstimator` 包装无训练参数的 raw-probability identity classifier，只从 calibration split 的 748 个 label 拟合单调 sigmoid。方法在看指标前固定，不用同一 calibration split 选择 isotonic；artifact 同时保存 raw/calibrated probability、`a`/`b`、输入模型 hash 和 calibration curve。原因是校准必须与 MODEL-004 train 拟合隔离，并避免更高自由度映射在当前样本上过拟合。影响是 calibration 指标只能视为 in-sample fit diagnostic；train/validation 的 calibrated 输出只用于映射重放，MODEL-006 才能给出 out-of-time 证据，final test 继续 sealed。

## 2026-08-13 — Walk-forward 使用 expanding train、独立 calibration 和后续 evaluation

MODEL-006 将公开 Development 构造成三个连续且不重叠的 evaluation fold；每个 fold 依次执行 expanding train、紧邻且独立的 calibration、以及更晚 evaluation。Constant/统计模型不读取 evaluation label，Elo 沿用逐场先预测后更新合同；整体、全部 fold、6 个赛区和 BO3/BO5 均完整报告。原因是固定 validation/calibration 指标不能证明跨时间稳定性，随机交叉验证会破坏赛前信息边界。影响是 959 场 Walk-forward 可作为 MODEL-007 的公开 Development 证据，但不 release 356 场 final test，也不自动形成 Gate 1 结论；Market Baseline 因 linked-only 母体和时间范围不同继续单独报告。

## 2026-08-13 — Gate 1 失败并停止当前模型路线

MODEL-007 在 Final Test release 前依据公开 Walk-forward 固定回退 sigmoid、选择 raw statistical，并冻结 Constant/Elo/raw/calibration/Walk-forward artifact、raw config、Gate config 与 evaluation code hashes。唯一一次成功主评估中，356 场 Final Test 的 raw 相对 Elo Brier/Log Loss 退化 `+0.0137266278/+0.0304206175`，超过预注册 `+0.01/+0.02` 灾难性退化线；公开加 Final 综合指标也劣于 Elo，因此 Gate 1 裁决 `failed_stop_modeling`。影响是 `BACK-001` 和整个 M4 不获授权，不得在看过本次结果后修改模型并复用同一 Final Test；恢复路线必须建立新的独立 out-of-sample cohort、新 version 与新 seal，并永久保留本次失败 artifact。

## 2026-08-13 — 旧 Final Test 退役并只授权新数据恢复路线

M3R-001 将已释放的 356 场 Final Test 永久标记为 `retired_diagnostic_evidence_never_independent_again`。完全相同的 325 场冻结模型在 959 个公开 evaluation IDs 上仍 3/3 folds 略优于 Elo，说明 expanding/frozen 训练协议差异不是公开优势符号的必要条件；Final BO5 占比由 `7.51%` 升至 `47.19%`，共同 `Region×BO` cell 的构成效应和 cell 内 residual 均为实质贡献，但无法因果识别具体 feature 或修复方案。影响是下一步只授权 `M3R-002` 建立 2025-07-01 之后的新 candidate corpus；旧 Final 禁止用于 feature/model/parameter/calibration 选择和未来 Gate。

## 2026-08-13 — 恢复候选必须以旧 corpus upstream 证明双重零重叠

M3R-002 固定 `[2025-07-01,2026-07-01)` 为新 source candidate 时间窗，并把旧 1,778 条 Series Result manifest/output 作为显式 upstream。Rust audit 对 reference/new `series_id` 和 Scheduled Start 同时 fail closed，只有 `member_overlap_count=0`、`temporal_overlap_count=0` 且 `max(old)<min(new)` 才能生成有效恢复语料。`Tournaments.OverviewPage -> Region` exact relation 在本阶段只统计 source coverage，不生成或暗示 Canonical Competition identity，也不合并不同 Region source value。原因是恢复 cohort 的独立性必须由实际旧成员和时间证据证明，而 Region 验收不能提前绕过 M3R-003 的时间化 identity 合同。影响是 3,759 candidates 仍不能计作 eligible Series Result；下一步必须重新执行 Team/Competition identity、结果与 `T-15m` 特征 lineage。

## 2026-08-13 — Tournaments Year 不参与 Historical Identity 判定

M3R-003 移除 HIST-010 identity builder 中仅适用于 2025H1 的 `Tournaments.Year == 2025` 硬编码。`Year` 只保留为来源描述字段；跨年度 Competition identity 仍必须由每条 candidate 的 `OverviewPage` 在 exact `Tournaments.OverviewPage -> League/Region` relation 中唯一解析，并与该 candidate 的 Scheduled Start 组合成事件时点 evidence。原因是年份既不能唯一标识赛事，也不能替代具体赛事页面关系。影响是同一 builder 可覆盖 2025/2026 candidates，但 Missing/Ambiguous 仍 fail closed，禁止 fuzzy、slug、source-key fallback 或按年份自动确认。

## 2026-08-14 — 恢复 split 必须排除旧 Final 的整个时间与成员空间

M3R-004 在普通 Temporal Split Manifest 上增加独立 recovery context：重新从旧 Feature Snapshot 计算旧 Final commitment，并要求旧 Final 与整个新 corpus 的 `member_overlap_count=0`、`temporal_overlap_count=0`，而不是只检查新 Final。新边界在不读取 feature value 或 label 的情况下按完整 UTC 月固定为 6/2/2/2 个月，形成 1,281/430/743/701。原因是只把旧成员从新 Final 移走仍会允许旧 holdout 进入训练和模型选择，破坏恢复 Gate 的独立性。影响是新 Final 继续只发布 count/window/commitment，aggregate coverage 可公开但不得驱动 M3R-005 选择；旧 Final 永久只作 diagnostic evidence。

## 2026-08-14 — Gate 1 恢复采用两速 Feature Lab 与 Elo-offset P0

本决策 supersede 旧开发计划中“10 维累计 form Logistic Regression 作为第一版主模型”的推荐语义，但不删除其历史结果。P0 固定以赛前 game Elo logit 为 offset，只学习对手质量校正、时间衰减 form、赛程密度和 availability 等少量 residual，再用确定性 BO3/BO5 DP 得到 series probability；unsupported cell 回退 Elo。现有 ScoreboardGames 若不能提供逐局 winner，则 series 完成后只能按最终 game counts batch update，不得伪造局序。Rust 保留 identity、cutoff、membership、seal 与 lineage，Python Feature Lab 负责实验列及 `source_max_at/input_count/status` 审计，只有晋升 FeatureSet 才支付完整冻结成本。原因是旧模型平行重学队伍强弱且 BO5 dummy 无法表达生成机制，而每列跨 Rust/DB/PowerShell/Python 同步会显著降低小样本研究迭代速度。影响是 P0 不引入 tree、roster/Patch micro-stat 或完整 Bayesian 层级模型，新 Final 在所有 P0 决策冻结前不得 release。

## 2026-08-14 — P0 未通过公开时间稳定性并停止在 Final release 前

M3R-005 的 1,173 条公开 Walk-forward evaluation 在自然构成汇总上相对生成式 Elo 略优，但 Fold 1/2 的 Brier 与 Log Loss 均劣化，Fold 4 只有 Brier 微弱改善；固定四窗共同 `Region×BO` 构成后 3/4 folds 双指标劣化。该证据反驳了“总体均值改善即足够晋级”的解释，因此状态固定为 `failed_public_stability_stop_before_final`。现有 ScoreboardGames 没有逐局 winner，P0 只使用可审计的 series-atomic game-count batch update，不伪造局序。影响是新 701 条 Final 继续 sealed，M3R-006 不获授权；不得通过搜索当前参数或删反例继续试验，下一步只能先作新增原子证据的 P1 Go/Kill 决策，默认保留生成式 Elo并停止统计恢复。

## 2026-08-14 — P1 evidence 不足并 Kill 统计恢复路线

M3R-005A 确认 Leaguepedia 当前 ScoreboardGames schema/Cargo 行包含逐局 winner 和 player fields，但“字段存在”不满足 prematch evidence 门禁。五个主要公开反例的目标实际 roster 在 `T-15m` immutable revision 中 0/5 可得；10/10 team-sides 的最近已发布 roster 虽可审计，却与赛后目标 lineup 10/10 相同，不能解释反例。固定 P0 `scale=400/K=20` 枚举全部合法 BO3/BO5 局序后，单个已完成系列赛的 sequential update 相对 batch update 对紧邻下一系列赛概率的一步影响小于 `0.0097` 且方向未证明；该局部界不声称约束多场累积 rating divergence。原因是 P1 Go 必须同时满足 `T-15m` 可得、秒级 `available_at` 可审计和具体反例针对性，不能由当前 schema、赛后 lineup 或 post-hoc 参数搜索替代。影响是裁决 `kill_recovery_model_keep_generative_elo`，不授权 roster/player、sequential Elo、Patch/micro-stat 或其他 P1；新 Final 继续 sealed，M3R-006、M4、策略、PnL 与执行保持阻塞。未来新 evidence 必须先建立独立任务合同，不能直接恢复本路线。

## 2026-08-14 — Actual lineup source feasibility 与 P1 authorization 分离

EVID-001 要求单一来源同时具备 Research 权限、目标 Game 1 实际五人首发语义、稳定 Event/Team/Player ID、秒级 `available_at`、T-15 capture 和 immutable raw，禁止把 Leaguepedia tournament roster 的时间能力与 postgame scoreboard 的实际阵容语义拼成虚构 feed。当前 Riot/GRID、Leaguepedia TournamentRosters/ScoreboardGames、Oracle's Elixir 和官方公告五类来源 eligible 为 0/5，因此裁决 `blocked_no_eligible_source`。原因是 source feasibility 未通过时直接写 collector 只会系统化采集弱证据；先冻结 28 天 China/Korea 全量 BO3/BO5 观察协议，则可防止未来按可用公告或阵容变化挑样本。影响是当前不启动 forward collection、不授权 P1 或 Final release；未来 source gate Go 也只进入 `ReadyForForwardCollection`，仍需覆盖、准确率和 changed-lineup 门槛后才能另行讨论 P1。

## 2026-08-14 — 官方首发公告必须按赛区独立完成 source registry

EVID-002 禁止用 LPL/LCK 的单个社交帖子直接启动采集，要求 China 与 Korea 各自至少有一条官方来源独立通过官方归属、Game 1 五人语义、目标赛区覆盖、稳定 permalink、无需登录稳定访问、秒级 `available_at`、稳定 Event/Team/Player ID 与 immutable raw。LPL 官方微博已证明中心公告语义，但访问、秒级时间、identity 和 raw 合同不完整；LCK 规则已证明联盟掌握并披露五人 entry，但逐场公开 channel 未定位。因此裁决 `blocked_registry_incomplete`。原因是“公告存在”不能证明系统性 feed，且两个赛区的来源机制不可互相外推。影响是 EVID-001 的 28 天窗口仍未授权；若未来接受 authenticated API，必须由新任务显式修改 login-free 合同并审核凭证、费用、retention 与 sample response，不能在现 registry 中直接翻转布尔值。
