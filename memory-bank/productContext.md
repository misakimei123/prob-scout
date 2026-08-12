# Product Context

ProbScout 是可扩展到多类事件的概率预测与预测市场验证系统，当前只研究职业 League of Legends 盘前系列赛胜负市场。

核心问题是：独立概率模型相对真实可成交成本是否存在可重复、扣除 fee、spread、slippage 和不确定性后仍为正的优势。Threshold Strategy 是高概率买入基准；Edge Strategy 只交易保守净 Edge。

当前不假定系统盈利。路线是历史研究、实时 Paper、受控真实 smoke、小资金实验，任何 Gate 证据不足都停止扩展。详细业务边界以 `docs/DEVELOPMENT_PLAN.md` 为准。
