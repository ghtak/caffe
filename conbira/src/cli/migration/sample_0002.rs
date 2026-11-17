use std::future::Future;
use std::pin::Pin;

use conbira::Error;
use espr::sqlxdb::Database;
use sqlx::Postgres;

use super::{Migration, Order};

pub(crate) struct InsertSamples;

impl Migration for InsertSamples {
    fn order(&self) -> Order { 2 }

    fn name(&self) -> &'static str { "insert sample data" }

    fn run<'a>(
        &'a self,
        database: &'a Database<Postgres>,
    ) -> Pin<Box<dyn Future<Output = conbira::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let mut connection = database.conn().await.map_err(|error| {
                Error::internal("Failed to acquire sample migration connection")
                    .at("cli.migration.sample_0002")
                    .cause(error)
            })?;
            sqlx::query(
                "INSERT INTO conbira_sample (id, name, created_at, updated_at)
                 SELECT id, 'sample ' || id, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
                 FROM generate_series(1, 10) AS id
                 ON CONFLICT (id) DO NOTHING",
            )
            .execute(connection.handle().conn())
            .await
            .map_err(|error| {
                Error::internal("Failed to insert sample data")
                    .at("cli.migration.sample_0002")
                    .cause(error)
            })?;
            Ok(())
        })
    }
}
