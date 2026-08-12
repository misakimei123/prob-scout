# ProbScout

ProbScout 是一个 Research First 的盘前预测市场机器人项目。当前先建立本地研究与 Paper Trading 所需的最小骨架；后续只在本地稳定性、数据用途、账户资格、平台访问资格和安全 Gate 全部通过后，按 `live_smoke` → 双策略小资金实盘的顺序逐级解锁。VPS 只用于后续长期无人值守，不是开始验证的前置条件。

## 本地运行

查看 CLI：

```powershell
cargo run --locked -- --help
```

创建或升级本地 SQLite，并执行健康检查：

```powershell
cargo run --locked -- health
```

默认配置位于 `config/default.toml`，可使用 `PROB_SCOUT__*` 环境变量覆盖。例如：

```powershell
$env:PROB_SCOUT__LOG_LEVEL = "debug"
cargo run --locked
```

## 本地质量检查

提交代码前依次运行以下三条命令：

```powershell
cargo fmt --check
cargo check --locked
cargo test --locked --lib
```

当前项目规模较小，library tests 就是最小且完整的测试范围。只有出现明确需求时才增加新的质量工具，不引入重型 lint 门禁。

开发阶段与当前进度见 [`docs/TASK_BREAKDOWN.md`](docs/TASK_BREAKDOWN.md)，完整路线见 [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md)。
