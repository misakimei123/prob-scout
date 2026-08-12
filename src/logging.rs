use tracing_subscriber::EnvFilter;

/// 初始化写入 stderr 的结构化日志。
///
/// 日志格式由配置控制；调用方必须确保事件中不包含 secret。
pub fn init(
    level: &str,
    json: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let filter = EnvFilter::try_new(level)?;
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr);

    if json {
        subscriber.json().try_init()?;
    } else {
        subscriber.compact().try_init()?;
    }

    Ok(())
}
