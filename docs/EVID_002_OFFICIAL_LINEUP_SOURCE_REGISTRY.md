# EVID-002 China/Korea 官方首发来源登记审计

> 审计日期：2026-08-14
>
> 状态：Completed / `blocked_registry_incomplete`
>
> 范围：只建立官方公告 source registry 并执行静态门禁；不启动 28 天采集、不训练模型、不读取或 release Final、不进入 M3R-006/M4

## 1. 结论

China 与 Korea 当前都没有一条官方来源独立通过八项静态能力门槛，`forward_collection_authorized=false`。这不是“官方从不公布首发”的结论，而是“现有证据不足以把官方公告当作可重复、可时间化、可追溯的数据 feed”。

- China：LPL 官方微博已经证明中心账号会在赛前发布 BO3/BO5 首发公告，且 permalink 稳定；但公开页面只显示分钟级时间，自动访问不稳定，官方 CLI/API 需要登录，当前也没有 canonical Event/Team/Player ID 与 immutable raw 合同。因此状态为 `conditional_api_probe_and_identity_contract_required`。
- Korea：2026 LCK 规则已经证明联盟办公室持有双方各五人的 match entry，并在双方到场后披露；但本次没有定位到逐场公开披露 endpoint、稳定 post ID、秒级时间与身份字段。因此状态为 `blocked_public_disclosure_channel_unresolved`。

EVID-002 完成的是来源登记裁决，不是持续采集。EVID-001 冻结的 28 天观察协议仍处于未授权状态。

## 2. 冻结门槛

每个目标赛区必须至少有一条来源，在不跨来源拼接能力的前提下同时证明：

1. 官方归属；
2. 对应目标系列赛 Game 1 的双方五人首发语义；
3. 覆盖目标赛区，而非单一队伍的偶发公告；
4. 稳定 post ID 或 permalink；
5. 无需登录且适合 Research 的稳定访问；
6. 秒级 `available_at`；
7. 稳定 Event、Team、Player source ID；
8. 原始响应可保存并以 hash 不可变复核。

某个账号“发过首发海报”、搜索引擎“偶尔能索引页面”、或平台文档“理论上有 API”，都不能单独通过该合取门槛。

## 3. 核心事实与来源审计

| Region | Source | 已证明 | 关键缺口 | 裁决 |
|---|---|---|---|---|
| China | LPL 官方中心微博 | 官方账号、赛前五人首发语义、跨队伍中心账号、稳定 permalink | 无登录稳定访问、秒级发布时间、稳定 Event/Team/Player ID、immutable raw | `conditional_api_probe_and_identity_contract_required` |
| China | LPL 各队官方微博 | 个别队伍可发布准确五人名单、稳定 permalink | 完整账号 registry、统一覆盖、稳定访问、秒级时间与身份合同 | `fallback_fragmented_not_registry_ready` |
| China | LPL 各队 X | 个别官方账号五人首发；X lookup 文档提供 `created_at` | 需要 app/Bearer Token、非全赛区 feed、缺 canonical event/player mapping 与 raw 样例 | `supplemental_not_complete` |
| Korea | LCK 联盟 entry 披露 | 官方规则定义双方五人 entry，并规定联盟办公室披露 | 未定位逐场公开 channel/permalink/API、秒级时间、稳定 IDs 与 raw retention | `blocked_public_disclosure_channel_unresolved` |
| Korea | LCK 各队 social accounts | 官方账号归属与平台 permalink 能力 | 未证明常规、跨队伍、T-15 前发布 exact Game 1 entry | `blocked_no_repeatable_lineup_pattern_evidence` |

LPL 中心账号的公开页面给出了稳定 post permalink，并明确写出下一日 BO3 的“首发名单”；页面展示时间为 `25-07-20 23:16`，即分钟粒度：[LPL 官方首发公告样例](https://www.weibo.com/5756404150/PC1x98y8x)。账号主页还能看到同一模式持续出现，但页面访问和索引表现并不构成稳定采集合同：[英雄联盟赛事官方微博](https://www.weibo.com/u/5756404150)。微博官方 CLI 页面明确要求微博登录，免费计划仅限本人数据，批量读取属于订阅能力；当前项目没有已授权的公共账号读取响应样例：[微博开放平台 CLI](https://open.weibo.com/cli/index)。

X 的官方 post lookup 文档提供 `created_at`，但读取公开数据要求 developer account、app 与 Bearer Token；因此它证明平台有秒级字段，不证明本项目已经有稳定 Research access：[Post lookup](https://docs.x.com/x-api/posts/get-posts-by-ids)、[Getting access](https://docs.x.com/x-api/getting-started/getting-access)。单个 LPL 队伍账号的五人公告只能作为语义样例，不能替代全赛区覆盖：[TOP Esports lineup post](https://x.com/TOP_Esports_/status/2043717738187964458)。

LCK 官方页面发布并链接 2026 官方规则。所链接规则把 match `entry` 定义为五名选手，并规定双方到场后由联盟办公室披露；这证明 exact Game 1 lineup 的官方业务语义存在，但页面本身没有给出逐场披露 feed：[2026 LCK 规则更新与官方规则链接](https://lolesports.com/ko-KR/news/2026-lck-rulebook)。当前 LCK 页面公开的是赛程、新闻和 integrated roster 等内容，未由本次审计定位出逐场 entry endpoint：[LCK 官方页面](https://lolesports.com/ko-KR/lolesports?leagues=lck)。

## 4. 可重复实现与结果

- `research/evid002_official_lineup_sources.json`：冻结两个目标赛区、八项门槛、五条候选来源、证据 URL 与下一证据需求；
- `research/evid002_official_lineup_sources.py`：校验 registry schema，逐来源计算缺失能力，并要求 China/Korea 各自至少有一条完整来源；
- `tests/test_evid002_official_lineup_sources.py`：覆盖当前双区阻塞、单区通过仍阻塞、双区各有完整来源才授权、能力不得跨来源拼接，以及重复 ID、未知 Region、缺失布尔能力的 fail-closed 行为。

实际输出为五条来源、China eligible `0`、Korea eligible `0`，裁决 `blocked_registry_incomplete`。Registry/code/canonical-LF-output SHA-256 分别为 `eee576ca1c2623fc740be68fcbc9db205658c91e9a50995936156dad81b0596d`、`4923fc8e39aeccd20c517be55b64e667a9713305fa85d195d107f34d85edf8bd`、`77ba55c8fa35c67f0b169ed78aa6ef496699e77820464d809ca3760442dd8919`。

## 5. 反方观点与不确定性

- 搜索引擎能读取部分微博 permalink，说明“完全不可公开访问”过强；但采集门槛要求的是无需登录的稳定访问与原始时间证据，偶发索引不满足该条件。
- 微博或 X 的授权 API 可能补齐秒级时间和 raw capture；当前没有凭证、付费授权或 sample response，不能预先把能力记为通过。若未来接受 authenticated access，需要新任务明确修改本次的 login-free 门槛，而不是静默放宽。
- LCK 规则要求披露 entry，意味着公开 channel 很可能存在于未被搜索索引的 broadcast、app 或现场流程；本次结论是 channel unresolved，不是 channel absent。只有 LCK 官方 endpoint 或可复核样例才能改变状态。

结论置信度：China “有明确首发公告但不满足数据 feed 门槛”为高；Korea “规则语义存在但公开 channel 未定位”为中等，因为未索引的官方渠道仍可能存在。

## 6. 后续边界

当前不写 collector，也不启动 28 天窗口。下一步只有出现新增外部证据后再建任务：

1. China：取得 LPL 中心 feed 的获准响应样例，证明秒级 `created_at`、raw bytes、稳定 post ID，并建立 Event/Team/Player exact mapping 与覆盖审计；
2. Korea：从 LCK 官方渠道定位逐场 entry 披露 endpoint，取得带稳定 permalink、秒级发布时间和双方五人身份的样例。

两区都通过静态 registry 仍只进入 `ReadyForForwardCollection`，不授权 P1、Final release、M3R-006、M4 或交易。
