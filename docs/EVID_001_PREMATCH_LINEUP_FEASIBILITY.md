# EVID-001 赛前实际首发证据可行性审计

> 审计日期：2026-08-14
>
> 状态：Completed / `blocked_no_eligible_source`
>
> 范围：只审计来源与冻结前瞻观察合同；不训练模型、不读取或 release Final、不进入 M3R-006/M4

## 1. 结论

当前没有一条已审计来源同时满足以下六项合取门槛：

1. 本项目获得明确的 Research 访问权限；
2. 字段语义是目标系列赛 `Game 1` 的实际五人首发，而不是赛事大名单或赛后统计；
3. Event、Team、Player 都有稳定 source ID；
4. 原始来源提供秒级 `available_at`；
5. 证据能在 `T-15m` 前发布并被本项目捕获；
6. 原始响应及其 hash 可以不可变保存和复核。

因此 EVID-001 不启动持续采集，也不授权任何 roster/player P1。当前裁决只阻塞现有来源路线，不证明“赛前首发永远不可获得”；未来来源发生实质变化时可以建立新任务复审。

## 2. 核心事实与来源审计

| Source | 通过项 | 关键缺口 | 当前裁决 |
|---|---|---|---|
| Riot Esports Data / GRID Fixtures | 官方服务描述 upcoming tournaments、teams、players、matches，并由 Riot operations 更新 | 当前公开说明仍要求通过 GRID 商业接入；没有公开确认目标 Game 1 实际首发和秒级字段发布时间 | `blocked_contract_and_semantics_unconfirmed` |
| Leaguepedia `TournamentRosters` + revisions | 结构化 player link、赛事关系和页面 revision 秒级时间可审计 | `RosterLinks` 是赛事 roster，可包含替补，不等于目标 Game 1 五人首发 | `rejected_tournament_roster_is_not_actual_lineup` |
| Leaguepedia `ScoreboardGames` + revisions | `Team1Players`/`Team2Players` 可确认实际出场阵容 | M3R-005A 的五个指定反例在 `T-15m` revision 中 0/5 存在目标 lineup；这是赛后事实路线 | `rejected_postgame_fact` |
| Oracle's Elixir game rows | 可在赛后确认 player rows | 年度文件没有 row-level、不可变、秒级 `available_at`，不能证明 T-15 可见 | `rejected_no_row_level_available_at` |
| 官方 team/league announcements | 个别公告可能同时包含首发和公开发布时间 | 来源异构、没有冻结 registry 与稳定 ID 合同，也没有跨 China/Korea 的完整性证据 | `probe_only_heterogeneous` |

Riot 当前官方页面说明 GRID Fixtures 提供 upcoming tournaments、teams、players 和 match times，同时说明 live、fixture、A/V 数据目前通过 GRID 面向商业用途提供；这只能证明产品能力，不能替代本项目的访问授权和字段语义确认：[Official Riot Esports Data](https://riotesportsdata.com/en-us/league-of-legends/)。

Leaguepedia 当前 schema 明确把 [`TournamentRosters.RosterLinks`](https://lol.fandom.com/wiki/Module:CargoDeclare/TournamentRosters) 定义为赛事 roster，而 [`ScoreboardGames.Team1Players/Team2Players`](https://lol.fandom.com/wiki/Module:CargoDeclare/ScoreboardGames) 是实际出场 players。两张表分别缺“实际首发语义”和“T-15 可得性”，不能拼接成一条虚构的 eligible feed。

## 3. 冻结的前瞻观察协议

只有未来某一来源先通过六项 source gate，才允许另建任务启动观察。协议已在 `research/evid001_prematch_lineup_config.json` 中冻结：

- 目标：China/Korea 的全部 BO3/BO5，目标事实固定为 `actual_game_1_starting_lineup`；
- 起点：source gate Go 且 config hash 冻结后，至少等待 72 小时，再取第一个 Monday `00:00 UTC`；
- 时长：连续 28 天，不按结果、队伍或阵容变化挑样本；
- 抓取点：`T-60m`、`T-30m`、`T-15m`；
- 赛后核验：只用 Game 1 scoreboard player links 确认预报首发是否准确；
- eligible observation：双方各 5 个唯一 player ID，队伍不相同，`available_at <= captured_at <= T-15m`，raw SHA-256 有效；
- 任何 source gate、时间、身份或完整性失败均标记 `rejected`，不降级为 last-known roster。

预注册 Go 门槛：至少 80 个 series；T-15 完整覆盖率整体不低于 90%，样本数不少于 10 的 `Region×BO` segment 不低于 80%；赛后确认首发准确率不低于 95%；至少 10 个相对 last-known roster 发生变化的 team-side；accepted observations 的身份歧义与时间违规均为 0。覆盖门槛防止把偶发公告当作系统性 feed，changed-lineup 门槛防止只在稳定阵容中得到没有 P1 增量的高覆盖率。

## 4. 可重复实现与验证

- `research/evid001_prematch_lineup.py`：验证 source registry 的六项合取门槛，并对未来 observation 执行 T-15、双方五人、ID 和 raw hash 的 fail-closed 审核；
- `research/evid001_prematch_lineup_config.json`：冻结来源判断、样本窗口触发规则和 Go 门槛；
- `tests/test_evid001_prematch_lineup.py`：覆盖当前来源全部阻塞、能力不得跨来源拼接、合法 observation、cutoff/身份/Region/BO 拒绝及重复 source ID。

实际运行得到 5 个 audited sources、0 个 eligible source，`forward_collection_authorized=false`。Python 3.12 下 7 个定向测试通过，Ruff 0.12.9 check/format 通过。

冻结 config SHA-256 为 `721ed109b6ab7364f0cb433a97e45be6285e3de118d1775801cbb6fa717d1b9c`，审计代码 SHA-256 为 `9692414c0c045897552b0be74348f234f36e1e7ad8a2cc289ed5400f8bae1b41`，canonical LF 标准输出 SHA-256 为 `b8c9bd606cb5c35585579530cfd5d62d2132a074473c48f4a648e0680e69b29a`。

## 5. 反方观点与不确定性

- GRID/Riot 的非公开合同或字段可能已经提供目标 lineup 与变更时间；当前没有访问证据，不能按公开产品介绍推断。若取得书面 Research 权限、schema 和 sample response，应新建任务重审，而不是修改本结论中的布尔值后直接采集。
- 某些官方 team/league channels 可能长期稳定地在 T-15 前公布首发；当前未建立完整 source registry，也没有证明“未发公告”与“采集失败”可区分。先做来源登记与身份映射，才有资格启动前瞻窗口。
- 本结论对已审计的结构化公开路线置信度高；对全部异构官方公告是否存在未发现的稳定 feed，证据不足，置信度中等。

## 6. 后续边界

当前不应写 collector、训练 P1、release 701 条 Final 或进入 M3R-006。下一步只有在获得新增外部证据并由用户授权后，二选一：

1. 审核 GRID/Riot 的书面访问、字段语义、coverage、retention 与 derived-data 权限；
2. 固定 China/Korea 官方公告 source registry、source/player identity 和 raw capture 方式，再启动冻结的 28 天观察窗口。

任一路线完成 source gate 只会进入 `ReadyForForwardCollection`，仍不等于 `GoForP1`。
