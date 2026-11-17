use std::sync::Arc;

// Espresso con birra
use clap::Parser;
use conbira::config::Config;
use conbira::{Error, app, option};

#[tokio::main]
async fn main() -> conbira::Result<()> {
    let options = option::Options::parse();
    let config = Config::load(&options.config, options.env_file.as_deref())?;
    let guard = espr::tracing::initialize(&config.trace).map_err(|error| {
        Error::internal("Failed to initialize tracing")
            .at("main.tracing.initialize")
            .cause(error)
    })?;
    let container = Arc::new(app::Container::new(&config).await?);

    {
        let mut conn = container.db.conn().await.map_err(|error| {
            Error::internal("Failed to connect to database")
                .at("main.database.connect")
                .cause(error)
        })?;
        let value: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(conn.handle().conn())
            .await
            .map_err(|error| {
                Error::internal("Failed to check database connection")
                    .at("main.database.check")
                    .cause(error)
            })?;
        tracing::info!(value, "database connection test succeeded");
    }

    container.cache.ping().await.map_err(|error| {
        Error::internal("Failed to check cache connection")
            .at("main.cache.check")
            .cause(error)
    })?;
    tracing::info!("cache connection test succeeded");

    let router = app::http::router(container.clone())?;

    container.http_server.run(router).await.map_err(|error| {
        Error::internal("Failed to run http server")
            .at("main.http.run")
            .cause(error)
    })?;

    guard.shutdown().await.map_err(|error| {
        Error::internal("Failed to shut down tracing")
            .at("main.tracing.shutdown")
            .cause(error)
    })?;
    Ok(())
}
