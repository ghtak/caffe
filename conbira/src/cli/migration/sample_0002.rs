use std::future::Future;
use std::pin::Pin;

use conbira::error::ResultExt as _;
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
            let mut connection = database.conn().await.internal(
                "Failed to acquire sample migration connection",
                "cli.migration.sample_0002",
            )?;
            sqlx::query(
                "INSERT INTO conbira_sample (name)
                 SELECT 'sample ' || n FROM generate_series(1, 10) AS n
                 WHERE NOT EXISTS (
                     SELECT 1 FROM conbira_sample WHERE name = 'sample ' || n
                 )",
            )
            .execute(connection.handle().conn())
            .await
            .internal("Failed to insert sample data", "cli.migration.sample_0002")?;
            Ok(())
        })
    }
}
