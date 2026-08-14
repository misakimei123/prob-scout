# M3R-005A P1 Evidence Go / Kill Decision

> 状态：Completed — `kill_recovery_model_keep_generative_elo`
>
> 固定日期：2026-08-14
>
> 范围：只审计公开 Development 的新增原子证据可得性；701 条新 Final Test 未 release、未推导成员、未计算指标。

## 1. 结论先行

M3R-005A 裁决为 **Kill**：不授权 roster/player、逐局 sequential Elo、Patch/micro-stat 或其他 P1 恢复候选；正式停止当前统计恢复模型路线并保留生成式 Elo 作为研究基准。

新增字段“存在”不等于满足 P1 门禁。Leaguepedia `ScoreboardGames` 当前 schema 和 Cargo 响应确实包含逐局 `Winner`、`Team1Players`、`Team2Players`，但目标实际 roster 在所审计的五个主要公开反例的 `T-15m` revision 中均不可得。最近一次已发布 roster 虽可在 cutoff 前审计，却在 10/10 team-sides 上与赛后确认的目标 lineup 完全相同，无法解释这些反例。单个已完成系列赛中，逐局胜负顺序相对现有 game-count batch update 对紧邻下一系列赛概率的一步影响小于 `0.0097`，显著小于五个反例中 P0 相对 Elo 的 `0.0807–0.1816` 概率位移，也没有证明修复方向。

因此没有同时满足“`T-15m` 可得、秒级来源可审计、预期修复具体反例”的新增原子证据。继续试验只会把已观察的四窗结果转化为 post-hoc 模型搜索。

## 2. 审计边界与反例选择

只读取 M3R-005 artifact 中已经公开的 Development predictions，不读取或推导新 Final 成员。反例预先限定为报告中两个主要劣化区域：Fold 2 Korea 与 Fold 4 China，再按单场正 Brier harm 选择前五场：

| Public Development series | Segment | `model - Elo` probability |
|---|---|---:|
| `LCK CL/2026 Season/Kickoff_Playoffs Round 2_2` | Fold 2 Korea | +0.1816 |
| `LCK/2026 Season/Cup_Playoffs Round 2_2` | Fold 2 Korea | +0.1046 |
| `LCK CL/2026 Season/Kickoff_Playoffs Round 2_1` | Fold 2 Korea | +0.1416 |
| `LPL/2026 Season/Split 2_Week 4_10` | Fold 4 China | -0.0807 |
| `LPL/2026 Season/Split 2_Week 2_3` | Fold 4 China | +0.1205 |

这里使用赛后已知 lineup 只作公开 Development 失败归因，不进入任何 Feature Snapshot，也不形成可部署 feature。

## 3. 候选来源事实

Leaguepedia [`ScoreboardGames`](https://lol.fandom.com/wiki/Module%3ACargoDeclare/ScoreboardGames) schema 声明逐局 `Winner`、`WinTeam`、`Team1Players`、`Team2Players`、`GameId`、`MatchId` 和开始/时长字段；[`ScoreboardPlayers`](https://lol.fandom.com/wiki/Module%3ACargoDeclare/ScoreboardPlayers) 另有 player、team、role、`PlayerWin` 与 `GameId`。2026-08-14 的小范围 Cargo probe 对上述五场及其最近先验系列赛均返回逐局 winner 和双方五人列表。

Oracle's Elixir 当前年度 CSV 也含逐局 `result` 和 player rows，但项目固定的源文件只记录整份文件的下载/捕获时间，没有每行历史发布 `available_at`。它可以证明赛后事实，不能单独证明目标赛事 `T-15m` 时这些字段已经可见。

## 4. `T-15m` revision 审计

对每个目标 `DataPage` 使用 MediaWiki revisions API，选择 `timestamp <= Scheduled Start - 15m` 的最后 revision，并在该 immutable revision content 中检查双方和十名赛后确认 player。结果为 0/5 可用：

| Series | Cutoff UTC | Pre-cutoff revision | Team visibility | Player hits |
|---|---|---:|---:|---:|
| LCK CL Playoffs Round 2_2 | 2026-02-23 07:45 | `4389323` @ 07:38:42 | 0/2 | 0/10 |
| LCK Cup Playoffs Round 2_2 | 2026-02-15 07:45 | `4382798` @ 2026-02-14 12:35:37 | 0/2 | 2/10 |
| LCK CL Playoffs Round 2_1 | 2026-02-23 04:45 | `4352324` @ 2026-01-10 19:40:07 | 0/2 | 0/10 |
| LPL Split 2 Week 4_10 | 2026-04-26 08:45 | `4433137` @ 2026-04-25 13:53:46 | 0/2 | 0/10 |
| LPL Split 2 Week 2_3 | 2026-04-09 08:45 | `4419108` @ 2026-04-08 15:23:15 | 1/2 | 5/10 |

反向检查最近一次已完成系列赛的 roster：10/10 team-sides 在目标 cutoff 前的 revision 中能找到完整五人，但与目标赛后 lineup 比较为 10/10 完全相同。结论不是“roster 永远无用”，而是本批新增 last-known roster 对已指定的主要反例没有增量解释力；目标实际 lineup 又不满足 prematch availability。

revision timestamp 是页面级可审计时间。未在对应 revision content 中出现的字段不能因为当前 Cargo 行存在而倒推为当时可得。

## 5. 逐局顺序的增量上界

`research/m3r005a_game_order_bound.py` 在固定 P0 `scale=400/K=20` 下枚举所有合法 BO3/BO5 胜负顺序，并扫描初始 rating difference `[-1200,+1200]`、步长 `0.1`。它只比较上一系列赛 sequential update 与现有 batch update 对下一系列赛生成式概率的影响，不拟合参数、不读取标签进行选择。

| Previous series | Next BO3 max `|Δp|` | Next BO5 max `|Δp|` |
|---|---:|---:|
| BO3 | 0.002192 | 0.002668 |
| BO5 | 0.007872 | 0.009619 |

五个主要反例的 P0–Elo 概率位移绝对值最小为 `0.0807`，至少是上述单步最大界的 8.4 倍。该枚举不声称约束多场历史累积后的全局 rating divergence；它证明的是目前只有一个较小、方向未定的局部机制，没有形成针对现有反例的正向证据。逐局顺序可能细化 rating，但现有证据不能支持它会按正确方向修复公开时间稳定性，更不能据此授权 player model 或新 Final release。

## 6. Go / Kill 矩阵

| Go 条件 | 证据 | 判定 |
|---|---|---|
| 新原子字段真实存在 | ScoreboardGames 有 winner/player fields | 通过 |
| 目标 evidence 在 `T-15m` 可得 | 五个主要反例实际 roster 为 0/5 | 失败 |
| `available_at` 秒级可审计 | revision 可审计，但目标字段在 cutoff revision 中不存在；OE 无 row-level `available_at` | 失败 |
| 能针对具体反例 | 10/10 last-known roster 与目标一致；game-order 仅有较小单步上界且方向未证明 | 失败 |
| 不依赖已观察窗口做参数搜索 | 本任务未搜索 half-life/K/L2/support threshold 或删分段 | 通过 |

Go 是合取门槛，不是多数投票。三项核心条件失败，因此必须 Kill。

## 7. 影响与重新打开条件

- 生成式 Elo 保留为研究基准，但这不等于模型有效、策略获批或可交易。
- 新 701 条 Final Test 继续 sealed；M3R-006 保持 Blocked，不执行一次性评估。
- M4、策略、PnL 和执行继续阻塞。
- 当前统计恢复路线结束，不再对现有 P0 做同源参数、窗口、分段或 feature-combination 搜索。
- 只有未来出现独立、不可变、带秒级 `available_at` 的赛前 lineup/roster feed，并在多个公开时间窗口覆盖主要反例类型，才可创建新的任务合同重新讨论 P1；这不自动恢复 M3R-006。

## 8. 验证

- `python -m unittest tests.test_m3r005a_game_order_bound -v`
- `python research/m3r005a_game_order_bound.py`
- 精确扫描新文档不含 Final member IDs 或 label
- `git diff --check`

来源访问遵守项目现有 Leaguepedia Cargo/API、attribution 与本地研究边界；未抓取渲染 HTML，未启动服务，未运行模型训练或 Final evaluation。
