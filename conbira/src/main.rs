use std::sync::Arc;

// Espresso con birra
use clap::Parser;
use conbira::config::Config;
use conbira::{app, option};

#[tokio::main]
async fn main() -> espr::Result<()> {
    let options = option::Options::parse();
    let config = Config::load(&options.config, options.env_file.as_deref())?;
    let guard = espr::tracing::initialize(&config.trace)?;
    let container = Arc::new(app::Container::new(&config).await?);

    {
        let mut conn = container.db.conn().await?;
        let value: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(conn.handle().conn())
            .await?;
        tracing::info!(value, "database connection test succeeded");
    }

    let _shutdown = guard.shutdown().await?;
    Ok(())
}
