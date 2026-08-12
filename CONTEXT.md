# ProbScout Prediction Trading

ProbScout 研究事件结果概率，并验证这些概率相对预测市场可成交价格是否具有可重复的优势。当前首个研究题材是职业 League of Legends 赛前胜负市场。

## Language

**Prediction**:
模型在明确的信息截止时间生成的事件结果概率；生成后不可因市场结果或后续信息被回写。
_Avoid_: AI opinion, pick, tip

**Market Quote**:
某一时刻订单簿中实际可成交的 bid、ask、depth 和费用条件，而不是页面展示概率。
_Avoid_: Market probability, displayed price

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
