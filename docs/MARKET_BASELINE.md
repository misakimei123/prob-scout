# MODEL-003 Market Baseline

更新日期：2026-08-13

## 1. 合同

Market Baseline 只消费人工确认 `Matched`、具有 `Market Resolution Link` 且在统一赛前 cutoff 前存在官方 price history 的赛事。当前信息时点固定为 CLOB `Game Start - 15 minutes`；双方 outcome 各自取不晚于该 cutoff 的最后一个 `{t,p}` point，再通过显式 link 的 outcome 顺序映射到 `team_1_win`。

`p` 原值不做归一化。当前 16 场 Development 样本的双方 price sum 均为 1，但这只是当前证据，不是通用假设。缺少双方可靠 point、出现未来 point、映射未人工确认、outcome 顺序冲突或 Market Resolution 与 Series Result winner 冲突时均 fail closed，不填充 0.5 或其他伪造概率。

## 2. 与可成交价格的边界

官方 `GET /prices-history` 的 `p` 只作为 Grade C 市场概率信号。它不是买入 `ask`、卖出 `bid`，也不包含 spread、size、depth、当时 fee 或 10U VWAP。因此 MODEL-003 只计算 Brier 与 Log Loss，不计算 ROI、PnL、slippage、fill 或任何 execution-sensitive 指标。

当前 Market Baseline 使用 2026 固定审核样本的 linked subset，而 Constant/Elo 使用 2025H1 的 1,778 场模型语料。两个母体的时间范围和样本成员不同，指标不得直接用于模型优劣比较。

## 3. 构建入口

```powershell
./research/build_market_baseline.ps1 -Version <new-immutable-version>
```

构建脚本固定选择彼此兼容的 HIST-007 linked Series Result、Market Resolution Link 与 HIST-005 Temporal Split，校验三份 Dataset Manifest v1，并记录 DATA-008/009 CSV 的 SHA-256。Python 入口对相同输入构建两次，只有 artifact hash 一致才落盘。

## 4. 真实构建结果

固定版本：`2026-08-13.68be155.model003-v2`

| Split | Series | Brier | Log Loss |
|---|---:|---:|---:|
| train | 3 | 0.1544916667 | 0.4658236962 |
| validation | 7 | 0.0833964286 | 0.3087825440 |
| calibration | 6 | 0.2553708333 | 0.7434727676 |

- DATA-008 审核范围：50 场，其中 `Matched` 29、`NeedsReview` 21。
- DATA-009：50/50 为 Grade C；本任务仅消费 16 场公开 Development linked series。
- 价格范围：`0.085`–`0.95`；最大 staleness 为 52 秒；晚于 cutoff 的 point 为 0。
- 兼容 split 的 7 场 final test 继续 sealed，artifact 不包含其 ID、label 或指标。
- 当前 2025H1 模型语料的 356 场 final test 未被 MODEL-003 读取或释放。

Artifact SHA-256：`6dd7db70e085070d3e910e30f2ee105e6222b958f6a01cd2cca2348183432d9a`

配置 SHA-256：`08036a8518b7762a67f7b59eeb40263ee3db1357450a8f9b81d35574b33e4e58`

## 5. 限制

- 样本只有 16 场 Development linked series，分 split 指标方差很大。
- Grade C point 的精确微观结构语义不足，不能反推出历史可成交价格。
- 当前市场样本与 2025H1 模型语料无重叠，MODEL-003 完成不代表 Gate 1 已通过。
- 后续 MODEL-004 不得把本 artifact 当作 Feature Snapshot 或训练输入；本轮未进入统计模型、策略或执行开发。
