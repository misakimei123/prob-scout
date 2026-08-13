# ProbScout Prediction Trading

ProbScout 研究事件结果概率，并验证这些概率相对预测市场可成交价格是否具有可重复的优势。当前首个研究题材是职业 League of Legends 赛前胜负市场。

## Language

**Prediction**:
模型在明确的信息截止时间生成的事件结果概率；生成后不可因市场结果或后续信息被回写。
_Avoid_: AI opinion, pick, tip

**Market Quote**:
某一时刻订单簿中实际可成交的 bid、ask、depth 和费用条件，而不是页面展示概率。
_Avoid_: Market probability, displayed price

**Event**:
ProbScout 内部识别的一场完整系列赛；它关联双方队伍和 BO 类型，但不会把来源含义不同的时间强行合并。
_Avoid_: Match, market event

**Event Source Evidence**:
某个数据源对 Event 提供的原始赛事 ID、双方原始队名，以及带明确语义的来源时间。
_Avoid_: Event alias

**Team Alias**:
某数据源中的队伍 ID 与原始名称到 ProbScout 内部队伍身份的显式对应关系。
_Avoid_: Fuzzy team match

**Canonical Team**:
跨数据源保持稳定的队伍身份；只有证据明确的名称变体或改名才共享身份，二队、青训队和无法确认的所有权变更默认独立。
_Avoid_: Normalized team name, team string

**Team Identity Period**:
某个 Team Alias 在明确时间区间内指向一个 Canonical Team 的来源证据；区间外、缺失或同时指向多个身份时均不解析。
_Avoid_: Timeless alias, fuzzy identity

**Canonical Competition**:
跨数据源保持稳定的联赛或杯赛品牌身份；赛季、阶段和单场系列赛不是新的 Canonical Competition。
_Avoid_: Event, tournament label

**Competition Identity Period**:
某数据源的联赛或杯赛标识及名称在明确时间区间内指向一个 Canonical Competition 的来源证据。
_Avoid_: Competition string match

**Series Result**:
一场完整 BO3/BO5 Event 的最终赛果记录。它保留赛前已知的 competition、region、Patch、Scheduled Start 和双方 Canonical Team，并在赛后补充完整比分与胜者；它不要求存在预测市场，逐局结果也不能伪装成 Series Result。
_Avoid_: Game result, feature row, market-required result

**Historical Series Candidate**:
通过 MatchSchedule 与 ScoreboardGames 结构校验、但仍只保存 Leaguepedia team/competition source key 的完整 BO3/BO5 候选；可用 `OverviewPage -> Region` exact relation 做描述性 source coverage，但这不等于 Competition identity 已解析。只有后续时间化 identity resolution 成功后才能成为 eligible Series Result。
_Avoid_: Eligible series, normalized team identity

**Identity Coverage Audit**:
在每条 Historical Series Candidate 的 Scheduled Start 时刻，用显式有效期 evidence 分别解析双方 Team 与 Competition，并输出 `Resolved`、`Missing` 或 `Ambiguous` 的可重放审计。它只量化 identity 覆盖，不创建 Canonical ID。
_Avoid_: Fuzzy matching, current-name backfill

**Identity Review Queue**:
将 Identity Coverage Audit 中相同 kind、source key 和失败状态的未解析 occurrence 聚合后的人工补证清单；保留首次/末次出现时间、次数与受影响 series，不能把队列项本身当作已确认映射。
_Avoid_: Auto-approved alias, unresolved identity registry

**Historical Identity Evidence**:
由 Leaguepedia exact TeamRedirects/Tournaments relation 与具体 MatchSchedule 赛事时点共同组成的时间化身份凭证；只授权证据事件的有效区间，缺失或一对多 relation 继续 fail closed。`Tournaments.Year` 只是描述字段，不是 identity key；跨年度构建仍必须使用赛事自身的 `OverviewPage -> League/Region` exact relation。
_Avoid_: Source-key fallback, slug identity, timeless current redirect, year-as-identity

**Candidate Rejection**:
历史候选因 BO 类型、比分/Winner、Patch、逐局数量/时间或必填字段不满足合同而被排除的可审计记录；拒绝原因必须保留，不能在采集查询中静默过滤。
_Avoid_: Dropped row, ignored match

**Feature Snapshot**:
在固定赛前 cutoff 对一个 Event 生成的不可变特征集合；目标合同不包含该 Event 的比分、winner 或 market resolution，且每个历史特征都记录最新来源时间。
_Avoid_: Current team stats, mutable feature row

**Team Form Observation**:
一支队伍在某个已完成 BO3/BO5 中形成的历史结果记录；只有完成时间不晚于目标 Feature Snapshot cutoff 时才能参与特征。未有时间化 identity evidence 时只按来源内精确 key 使用，不猜测跨名称合并。
_Avoid_: Timeless team history, fuzzy historical identity

**Temporal Split Manifest**:
按互不重叠的半开 Event 时间窗口固定 train、validation、calibration 和 final test 归属的不可变合同；禁止随机打散和同一 series 跨集合。
_Avoid_: Random split, row split

**Final Test Seal**:
调参阶段只公开 final test 的时间窗、数量和成员 SHA-256 commitment，不公开成员 ID；模型、配置与评估代码冻结后才允许显式 release。
_Avoid_: Hidden flag, test set label

**Constant Baseline**:
只用 train split 的二元 label 总体比例，对所有赛事输出同一个 `team_1_win` 概率的无特征基准；validation/calibration 只评估，final test 在冻结前保持 sealed。
_Avoid_: 50% hard label, all-split prior, market baseline

**Elo Baseline**:
按 `(Scheduled Start, series_id)` 顺序对 development Series Result 逐场先预测后更新的全局队伍 rating 基准；首次参赛使用固定初始 rating，跨赛区不重置，同队同一开赛时刻的多场记录因结果先后不可证而 fail closed。
_Avoid_: Post-match prediction, region reset, unordered Elo

**Market Baseline**:
对人工确认 `Matched` 且具有 Market Resolution Link 的赛事，在统一赛前 cutoff 分别选取双方官方 price history 的最后一个 `p`，再按显式 outcome 顺序映射到 `team_1_win` 的 Grade C 概率信号基准；它不是可成交 ask，也不支持历史 PnL。
_Avoid_: Executable market price, ask baseline, historical fill

**Raw Statistical Probability**:
只在 train split 上拟合的简单可解释统计模型对公开 Development Feature Snapshot 输出的未校准 `team_1_win` 概率；validation/calibration 只评估或留给后续校准，final test 在冻结前保持 sealed。
_Avoid_: Calibrated probability, final model, causal effect

**Calibrated Statistical Probability**:
以冻结的 Raw Statistical Probability 为唯一输入、只用公开 calibration split label 拟合的单调概率映射；calibration split 指标属于拟合诊断，必须由后续 Walk-forward 才能形成 out-of-time 证据。
_Avoid_: Retrained model, final-test calibration, guaranteed frequency

**Walk-forward Evaluation**:
按时间重复执行 expanding train、独立 calibration 和更晚 evaluation 的公开 Development 评估；每个 evaluation series 只出现一次，并完整报告所有时间、赛区和赛制分段，不挑选最好窗口。
_Avoid_: Random cross-validation, final-test evaluation, best-window report

**Gate 1 Final Decision**:
在模型、配置、校准、Walk-forward 与评估代码全部冻结后，只对 sealed Final Test 执行一次主评估形成的继续、回退或停止裁决；失败结果必须永久保留，不得修改模型后复用同一 Final Test。
_Avoid_: Repeated final-test tuning, best-after-test selection, strategy authorization after failure

**Retired Final Test**:
已完成一次性 Gate 主评估的 Final Test；之后只允许作为失败归因的 diagnostic evidence，不再具备独立验证资格，禁止用于模型、特征、参数、校准选择或未来 Gate。
_Avoid_: Recycled holdout, post-test tuning set, second final evaluation

**Recovery Cohort**:
时间上严格晚于已退役 Final Test、成员零重叠且重新执行 identity/result/feature/split lineage 的新数据批次；只有新的 sealed Final Test 才能支持恢复 Gate。
_Avoid_: Extended old final, shuffled old corpus, renamed holdout

**Data Quality Gate**:
在模型开发前对 eligible series 数量、时间/赛区/Patch 覆盖、关键字段缺失、时间防泄漏、异常分布和历史市场真实性等级作出的可重复判定。`NotReadyForM3` 表示数据构建任务可以完成，但模型阶段仍被证据门禁阻塞。
_Avoid_: Report completed, pipeline passed

**Result Evidence**:
证明 Series Result 最终比分与胜者的来源记录。当前使用 Leaguepedia `MatchSchedule` 的系列赛比分/胜者，并用 `ScoreboardGames` 校验逐局数量和 Patch；身份映射成功本身不构成 Result Evidence。
_Avoid_: Identity mapping, market price

**Market Resolution Evidence**:
独立证明 Match Winner 市场最终结算 outcome 的来源记录。它必须明确市场已关闭并 resolved，且唯一获胜 outcome 对应的 Canonical Team 与 Series Result 胜者一致。
_Avoid_: Last traded price, identity mapping

**Market Resolution Link**:
按 `(series_id, market_id)` 将一个纯 Series Result 与已校验的 Market Resolution Evidence 关联的可选记录；缺少 link 不淘汰 Series Result，但该赛事不得进入 Market Baseline、Edge Strategy 或 PnL 分析。
_Avoid_: Nullable market fields on Series Result, inferred market outcome

**Market Mapping**:
一个 Event 与一个 Polymarket Match Winner 市场及其有序 outcome/token 的可追溯关联。
_Avoid_: Market match

**Scheduled Start**:
赛事数据源声明的系列赛计划开始时间。
_Avoid_: Market end

**Market End**:
Gamma 目录提供的市场事件结束时间；它不是比赛开赛时间的替代字段。
_Avoid_: Game start

**Game Start**:
CLOB 市场元数据中的 `gst`；盘前门禁仍需与 Scheduled Start 交叉核验。
_Avoid_: Gamma endDate

**Threshold Strategy**:
当 Prediction 达到最低概率阈值时产生交易候选的基准策略。
_Avoid_: Simple Bot

**Edge Strategy**:
仅当 Prediction 与 Market Quote 之间的保守净优势达到阈值时产生交易候选的策略。
_Avoid_: Alpha Bot

**Entry Policy**:
决定是否建立新仓位以及建立多少仓位的规则。
_Avoid_: Strategy logic

**Exit Policy**:
决定持有到结算、减仓或提前平仓的独立规则；其效果必须与 Entry Policy 分开验证。
_Avoid_: Stop loss

**Prematch**:
比赛官方开始前、在预先规定的信息截止时间进行预测和交易的阶段。
_Avoid_: Before game

**Intermap**:
一个小局结束后、下一个小局开始前重新估计系列赛结果概率的阶段。
_Avoid_: Inplay

**Inplay**:
单个小局进行期间，使用实时比赛状态重新估计条件胜率并交易的阶段。
_Avoid_: Live score trading

**Paper Account**:
使用真实时间戳和可成交盘口模拟成交，但不向交易场所发送订单的独立策略账本。
_Avoid_: Demo balance, fake account

**Trading Bankroll**:
专门分配给真钱策略、可承受全部损失且不与日常资金混用的资金。
_Avoid_: Startup capital, account balance
