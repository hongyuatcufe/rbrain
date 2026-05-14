use crate::config::Config;
use tracing_subscriber::EnvFilter;

pub fn init_logging(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let format = std::env::var("RBRAIN_LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

    match format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(env_filter)
                .init();
        }
    };

    Ok(())
}
