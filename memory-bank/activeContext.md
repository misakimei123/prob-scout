# Active Context

更新日期：2026-08-14

## 当前状态

- 分支：`main`，直接向 `origin/main` 推送。
- M0 已完成；M1 已完成 `DATA-001` 至 `DATA-010`，Gate 0 结论为 `Conditional Go`；M2 的 `HIST-001`–`HIST-010` 已实现，数据就绪 Gate 为 `ReadyForM3`。
- `DATA-008` 已固定并人工核验 50 场 recent historical Match Winner：5 分钟容忍值下 29 个 `Matched`、21 个 `NeedsReview`，自动 `Matched` 错误为 0。
- `DATA-009` 对同一 50 场的双方共 100 个 token 查询 T-15m 前 24 小时官方 `{t,p}` price history；A `0/50`、B `0/50`、C `50/50`、Unavailable `0/50`。
- `DATA-010` 允许 M2/M3 的 Grade C 信号研究；不授权历史可成交 PnL、实时盘口稳定性或真钱结论。DATA-005 只有单市场 REST snapshot，WebSocket、持续采集与断线恢复尚未实现或验证。
- `HIST-001` 已建立 `data/raw/`、`data/processed/`、`artifacts/` 目录合同和 Dataset Manifest v1；processed output 必须回溯 raw hash、UTC 生成时间、Git commit/dirty diff、生成入口、output hash、row count 和 Event 时间范围。
- `HIST-002` 已建立带半开有效区间的 Canonical Team/Competition identity registry；12 组显式队名变体、21 组赛事 label mapping（17 个 canonical competitions）可回溯 DATA-008 review ID。本批 verified rename periods 为 0。
- `HIST-003` 已从固定 50 场审核基线生成 23 行 BO3/BO5 Series Result：29 个 `Matched` 中排除 6 个 BO1，21 个 `NeedsReview` 继续排除；21 场 BO3、2 场 BO5 的必填字段完整，Leaguepedia winner 与 Gamma resolution 23/23 一致。processed dataset SHA-256 为 `04ba36d93f8560d9d0ece628cc372ebcebac58f70e71e7674eb37fb25db9bf95`。
- `HIST-004` 已对 23 场目标生成固定 `T-15m` Feature Snapshot：180 天 Leaguepedia 查询 1,761 行，形成 855 个精确 source-key team observations，16 个不完整 series fail closed 排除；23/23 快照有历史来源，晚于 cutoff 的来源时间为 0。dataset SHA-256 为 `f13e74dd8c3b28d888075ad4fb6ac4616aa34c6c62049e7f4db323e31a76a2fb`。
- `HIST-005` 已按连续半开 UTC 日期窗口将 23 场划为 train 3、validation 7、calibration 6、final test 7。调参 manifest 只公开前三组 16 个 ID；final test 只保存 count、window 和 membership SHA-256 `c1965fff7cbeb0cece75b7a1e4429c1d9e7b65699b1aa8cf6cff44811137583e`，release 需要冻结 model artifact/config/evaluation code hash。
- `HIST-006` 已生成缺失率、时间/赛区/Patch、IQR 异常与 Grade C 边界报告。必填 Series/Feature 缺失 0，source-time leakage 0/46，Same-Patch unavailable 3/46，execution-grade market snapshot 缺失 50/50；报告 SHA-256 为 `eddd8534144ffdcd9a1ec0a15052395922a7c3675ede12dc768af1982f8a86a2`，同输入双构建一致。
- `HIST-007` 已将纯 `Series Result` 与可选 `Market Resolution Link` 解耦。无市场的可靠 BO3/BO5 可进入 Constant/Elo/统计模型语料；Market Baseline、Edge Strategy 与 PnL 仍只接受 linked 子集。固定 23 场的 marketless/linked 构建得到相同纯结果 SHA-256 `336f48a31f313bedce04b499865b7a7bd10657adf7774808cafae1a274ae5a8c`，linked 模式另生成 23 行 market link。
- `HIST-008` 已建立 2025 上半年 Leaguepedia source-identity 候选 corpus：9,935 个 MatchId 中 2,061 个结构完整 BO3/BO5、7,874 个显式 rejection；候选覆盖 170 个 UTC 日期、13 个 Patch source key、468 个 team source key 和 146 个 competition source key。dataset SHA-256 为 `e80c7dcdff55b5f9c0b92e1669e6e95fdbb1a81a8c35bee339cad7ff7b43daa5`。
- `HIST-009` 已对 2,061 candidates 执行 Scheduled Start 时点的时间化 identity coverage。现有 HIST-002 evidence 只在 2026-08 DATA-008 观测秒内有效，2025H1 得到 fully resolved 0、blocked 2,061；team 为 0/4,122/0、competition 为 0/2,061/0（Resolved/Missing/Ambiguous），聚合成 614 条 review queue（468 team、146 competition）。dataset SHA-256 为 `a868952c4e6e1b0872d5786faa338d5c52dcefed724b15a9969252e263529b82`。
- `HIST-010` 用 exact `TeamRedirects.AllName -> canonical page` 与 `Tournaments.OverviewPage -> League/Region` relation，结合每条 MatchSchedule 赛事时点建立 1 秒 identity period；370/468 team keys 和 146/146 competition keys resolved，得到 1,778 fully resolved、283 blocked、98 条剩余 review queue。Identity dataset SHA-256 为 `e01d8a1fbcf547db23cff33b285a00a95cd663d42953fffde06069931a70fe50`。
- HIST-003–HIST-006 已重建为 1,778 Series/Feature rows、325/349/748/356 split；覆盖 170 个 UTC 日期、13 Patch、6 Region，3,556 个 team-side source time leakage 为 0。Series/Feature/Split/Quality SHA-256 分别为 `9e7a1c2d23b13570f16329e733a13457c997826bbde9fcb6fa2ce0c00334ae99`、`3a29cbfc7a9311b6bf36837da0fc2c24df115175460251bab862c6de89d50ab3`、`1ff428ae74f1a4a7d32dc033244f0aa74ff6268a818303258a7a96c01d699258`、`9a32f02e0e1a348ce01a7603163b8ac55bb14bdfd59975f2d40852cd45b92342`。
- 最近验证：Identity/Series/Feature/Quality 重放一致；33 个 identity raw、40 个 feature raw、全部 upstream/output hash 与五层 manifest 引用一致。全仓 84 个 Rust tests、`cargo fmt --check`、`cargo check --locked`、三个变更脚本 parser 与 `git diff --check` 通过。M2 Gate 依据预注册 1,778/500 volume 硬门槛更新为 `ReadyForM3`；单一年份、41.09% same-Patch unavailable 和 50/50 Grade C market evidence 仍是明确限制。
- `MODEL-001` 已使用 scikit-learn `DummyClassifier(strategy="prior")` 实现训练期总体先验 Constant Baseline。只消费 325 条 train label，得到 `P(team_1_win)=0.5230769231`；validation 349 条 Brier/Log Loss 为 `0.2479537478/0.6890521453`，calibration 748 条为 `0.2474473942/0.6880387182`。356 条 final test 未读取成员、未计算指标，artifact SHA-256 为 `39e55ce8f3f5e17bf69ba9c44c6eba994336e1738cc608aeb4431d49b940b3b2`。
- `MODEL-002` 已实现固定 `1500/400/K20` 的全局 chronological Elo Baseline。1,422 条 development 逐场先预测后更新，覆盖 319 个 Canonical Team；train/validation/calibration Brier 为 `0.2422573700/0.2427027093/0.2217084843`，Log Loss 为 `0.6775746090/0.6784341773/0.6348542326`。356 条 final test 未读取成员、未计算指标，artifact SHA-256 为 `49e71bdbc29b19f964cdd4f7db08f7f46d6b21eff981f566efd2541590255a40`。
- `MODEL-003` 已实现固定 CLOB `Game Start - 15m` cutoff 的 Market Baseline。只消费 16 条公开 Development linked series，按显式 outcome 顺序选择双方最后一个 Grade C `{t,p}` point；train/validation/calibration Brier 为 `0.1544916667/0.0833964286/0.2553708333`，Log Loss 为 `0.4658236962/0.3087825440/0.7434727676`。兼容 split 的 7 条 final test 保持 sealed，当前 2025H1 corpus 的 356 条 final test 未消费；artifact SHA-256 为 `6dd7db70e085070d3e910e30f2ee105e6222b958f6a01cd2cca2348183432d9a`。
- `MODEL-004` 已实现 scikit-learn train-only `StandardScaler + LogisticRegression`。10 个可解释输入均为双方 `T-15m` form 差值/availability/BO5 标记；325 条 train 拟合，validation/calibration 只评估 raw probability。train/validation/calibration Brier 为 `0.2211313304/0.2383272878/0.2193676117`，Log Loss 为 `0.6345254046/0.6712635152/0.6294449007`；356 条 final test 保持 sealed，artifact SHA-256 为 `7035396395c726232fe07e5b119b5d7c4cf0b39d60fef2fa2a2a77a789ba2611`。
- `MODEL-005` 已固定消费 MODEL-004 v2 raw probability，用 scikit-learn `CalibratedClassifierCV(method="sigmoid") + FrozenEstimator` 只在 748 条 calibration label 上拟合。calibration fit diagnostic 的 raw/calibrated Brier 为 `0.2193676117/0.2161185633`，Log Loss 为 `0.6294449007/0.6221717139`；10-bin curve、1,422 条 raw/calibrated Development prediction 和可重放 `a/b` 已写入 artifact。356 条 final test 保持 sealed，artifact SHA-256 为 `3ba241cbcbfbd397591daf7d8f0f7cefb905c46f5940928f4b0692aa95ea16df`。
- `MODEL-006` 已完成 3 个 expanding train / disjoint calibration / later evaluation fold，共 959 场不重叠 evaluation。整体 Elo/raw/calibrated Brier 为 `0.2264871457/0.2240006321/0.2241384269`，Log Loss 为 `0.6447592740/0.6393848851/0.6399372269`；raw 在 3/3 fold 略优于 Elo，但 Americas、China、BO5 均差于 Elo，sigmoid 只在 fold 1 改善 raw。整体、时间、6 Region、BO3/BO5 与 10-bin curve 已完整输出，356 条 final test 保持 sealed，artifact SHA-256 为 `bd08f5694d8c81b33b18af336614a29488e2ba7015c7274fadc62d74f25c9c4f`。
- `MODEL-007` 已在 release 前固定回退 sigmoid 并选择 raw statistical，冻结全部 artifact/config/evaluation hashes 后只成功执行一次 356 场 Final Test。raw/Elo Brier 为 `0.2500447064/0.2363180786`，Log Loss 为 `0.6953063265/0.6648857090`；raw 退化 `+0.0137266278/+0.0304206175`，超过预注册保护线，Gate 1 为 `failed_stop_modeling`。artifact SHA-256 为 `8380bb33219277e8404dd9b07c28ecda00aa19e27d1d09cad96f39ffd406af37`。
- `M3R-001` 已使用完全相同的 325 场冻结候选重放 959 个公开 evaluation IDs，仍在 3/3 folds 略优于 Elo；因此 expanding/frozen 训练协议差异存在但不足以解释 Final sign reversal。Final BO5 share 从 `7.51%` 升至 `47.19%`，共同 `Region×BO` cells 的 Brier composition/residual 为 `+0.0077955435/+0.0084671032`，两者均有实质贡献；18 场 `China|BO5` 无公开参照。旧 Final 已永久退役，归因 artifact SHA-256 为 `ba126c4ea192f4078f8795646796fa37cf5a2503a9f0cd7a89c59cd7e543271c`。
- `M3R-002` 已建立 `[2025-07-01,2026-07-01)` 非重叠 source candidate corpus：16,598 个 MatchId 得到 3,759 candidates、12,839 rejections，覆盖 349 个 UTC 日期、25 Patch、9 个 Region source value、BO3 2,819 / BO5 940。旧 1,778 条结束于 `2025-06-30T18:30:00Z`，新 candidate 起于 `2025-07-01T16:00:00Z`，member/temporal overlap 均为 0；103 个 raw page、旧 corpus upstream 和 output hash 已全量复核。dataset SHA-256 为 `f5c4210a04417392c92801a8d5f9e7d6c2b7c9f2871e63bd6e89d77f3d32860b`。
- `M3R-003` 已将 identity builder 泛化为跨年度 exact relation 审核，但没有放宽 identity：`Tournaments.Year` 仅为描述字段，Competition 仍由候选赛事时点的 `OverviewPage -> League/Region` relation 证明。3,759 candidates 得到 3,155 fully resolved、604 blocked；202/694 team keys Missing、267/267 competition keys Resolved、Ambiguous 为 0。3,155 条 Series Result 与 `T-15m` Feature Snapshot 成员完全一致，与旧 M2 corpus overlap 为 0，source-time/snapshot-lead/赛后字段 leakage 均为 0；Identity/Series/Feature SHA-256 分别为 `8f3e7aeadc9cf071adbe21fd74becd52126cd720fbe017b45b4755964d7bb331`、`dff9c9ee61cabf0c3a5a0a6aa9518fcd02cf6d28aa02a1cae6d6cd6a7817e6ac`、`8433cc10ee73cab042049d0afe0f81cfc0d96504348346178fb6c4baaa3c7f2b`。
- `M3R-004` 已用独立 recovery context 建立新 split：Train/Validation/Calibration/sealed Final 为 1,281/430/743/701，窗口依次为 2025-07–12、2026-01–02、2026-03–04、2026-05–06。旧 356 场 Final 的 commitment 已从旧 Feature Snapshot 重算，与整个新 corpus 的 member/temporal overlap 均为 0；新 Final 只公开 count 与 commitment `d8b3f5e2cca5eb707173a1ea4a8881c0b9e764173e6d24a373899639fab3a130`。split SHA-256 为 `ed7564bf68a4e16400c1d712242861a03a32893ecad5a91d814c86c1dcba64b1`，aggregate-only coverage 不输出 final IDs 或 label。
- `M3R-005` 已实现 P0 两速 Feature Lab、series-atomic game-count Elo、fixed game-Elo-logit offset residual 与 BO3/BO5 DP。2,454 个 model rows、39,264 个 audit rows 的 source-time violation 为 0；1,173 条公开 Walk-forward 汇总相对 Elo 的 Brier/Log Loss delta 为 `-0.00115008/-0.00180314`，但 Fold 1/2 双指标劣化、Fold 4 Log Loss 劣化，固定共同 `Region×BO` 构成后 3/4 folds 双指标劣化。状态为 `failed_public_stability_stop_before_final`，273 条 fallback，新 701 条 Final 未 release；artifact SHA-256 为 `f4e4892ca5daffd5edb1bfc2b785cf74cb8bb8fcc26860c2ab058c8d441a2144`。

## 下一任务

下一任务是 `M3R-005A` 决定是否存在足够新增原子证据授权 P1，默认选择停止统计恢复并保留生成式 Elo。不得 release 或推导 701 条 sealed Final 成员，也不得因已看到 P0 四窗结果而搜索 half-life、Elo K、L2、support threshold、删分段或同源 feature 组合。只有先证明真实 Game Result / roster availability 等新 evidence 的 `T-15m` 秒级可得性和针对性，才可另行批准 P1；M3R-006、M4、策略、PnL 与执行继续阻塞。

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
- Series Result 以 Leaguepedia `MatchId` 为键；重复候选只有核心赛事事实完全一致才按证据键稳定合并，任一比分、winner、Patch、时间、队伍或 competition 冲突都 fail closed。
- Market Resolution Link 以 `(series_id, market_id)` 为键并保持可选；缺少 link 不淘汰 Series Result，但该赛事不得进入 Market Baseline、策略或 PnL。存在 link 时任何 outcome、0/1 resolution 或 winner 冲突都 fail closed。
- Historical Series Candidate 只保存 Leaguepedia 原始 team/competition source key、完整比分/Patch 和完成时间；candidate 数量不得当作 eligible 数量。MatchSchedule 与 ScoreboardGames 分开采集，缺 game 必须成为 rejection。
- M3R-002 的 `OverviewPage -> Region` 只用于描述性 source coverage，不创建 Canonical Competition identity；原始 `Americas`/`North America` 等 value 不在 candidate 层合并。恢复 corpus 必须同时固定旧 corpus upstream，并在 Rust audit 中证明 member/temporal overlap 为 0。
- Feature Snapshot 固定 `T-15m`；目标合同不接收比分、winner 或 market resolution。历史结果只有最后一局结束时间不晚于 cutoff 才可使用，每个特征记录最新来源时间。
- HIST-004 历史 form 只按 Leaguepedia 精确 source key；不得把当前 Canonical Team identity 向历史外推。rename/alias 只有补齐带有效区间的 HIST-002 evidence 后才能合并。
- HIST-005 只按 Scheduled Start 使用 `[start,end)` 连续时间窗，不随机打散、不按小局拆分；同一 series 必须唯一命中一个集合。
- 调参只消费 train/validation/calibration IDs；final test 只发布 membership commitment。显式 release 前必须冻结 model artifact、model config 和 evaluation code 三个 SHA-256。
- HIST-006 的任务完成不等于 M2 Gate 通过；`NotReadyForM3` 时 MODEL-001 仍被阻塞。Same-Patch 无历史是显式 unavailable，不是 0% 胜率；IQR outlier 只进入 review，不自动删除。
- 要关闭实时稳定性条件，后续必须跨多个未来市场验证持续 order book 采集与离线重放；WebSocket 路径需覆盖订阅恢复、断线重连、全量重新同步、乱序和重复事件。
- processed dataset 必须位于 `data/processed/<dataset>/<version>/` 并有同目录 manifest；raw 输入不可原地修改，artifact 后续必须引用 dataset manifest 路径和 hash。
- source ID 存在时身份解析只按 ID；未知 ID 不回退名称。无 ID 时只按已登记的 source/name/observation time；`Missing`/`Ambiguous` 必须人工处理或排除。
- Identity Coverage 的 candidate row count 不等于 eligible count；只有双方 team 和 competition 在 Scheduled Start 时刻全部 `Resolved` 才能进入后续结果构建。2026 evidence 不得倒推覆盖 2025。
- `ReadyForM3` 只授权概率信号模型开发，不证明模型有效性或 execution readiness；Market Baseline/Edge/PnL 仍须 linked market evidence，Grade C 不能证明可成交性。
- Academy、Challengers、二队默认独立；赛事品牌、season/stage 和单场 Event 不得折叠为同一概念。

## 首次检查

进入新会话后先运行 `git status --short --branch` 和 `git log -3 --oneline`，确认远程提交与工作区状态，再读取 `docs/RECOVERY_MODEL_P0.md`、`docs/RECOVERY_TEMPORAL_SPLIT.md`、`docs/TASK_BREAKDOWN.md` 与 `CONTEXT.md`。下一任务只执行 `M3R-005A` P1 evidence Go/Kill 决策；未经新证据授权不继续建模，不 release 新 Final、不重跑 MODEL-007、不开始 `BACK-001`。
