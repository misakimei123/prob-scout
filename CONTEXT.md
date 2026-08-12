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
一场完整 BO3/BO5 Event 的最终赛果记录。它保留赛前已知的 competition、region、Patch、Scheduled Start 和双方 Canonical Team，并在赛后补充完整比分与胜者；逐局结果不能伪装成 Series Result。
_Avoid_: Game result, feature row

**Result Evidence**:
证明 Series Result 最终比分与胜者的来源记录。当前使用 Leaguepedia `MatchSchedule` 的系列赛比分/胜者，并用 `ScoreboardGames` 校验逐局数量和 Patch；身份映射成功本身不构成 Result Evidence。
_Avoid_: Identity mapping, market price

**Market Resolution Evidence**:
独立证明 Match Winner 市场最终结算 outcome 的来源记录。它必须明确市场已关闭并 resolved，且唯一获胜 outcome 对应的 Canonical Team 与 Series Result 胜者一致。
_Avoid_: Last traded price, identity mapping

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
