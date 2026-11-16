use super::*;

pub type TestDB = Database<sqlx::Postgres>;
pub type TestHandle<'c> = Handle<'c, sqlx::Postgres>;

pub mod sample {
    use super::{TestDB, TestHandle};

    async fn setup(handle: &mut TestHandle<'_>) -> crate::Result<()> {
        sqlx::query(
            "CREATE TEMPORARY TABLE sample (\
                id BIGINT PRIMARY KEY,\
                name TEXT NOT NULL,\
                created_at TIMESTAMPTZ NOT NULL,\
                updated_at TIMESTAMPTZ NOT NULL\
            )",
        )
        .execute(handle.conn())
        .await?;
        Ok(())
    }

    async fn teardown(handle: &mut TestHandle<'_>) -> crate::Result<()> {
        sqlx::query("DROP TABLE pg_temp.sample")
            .execute(handle.conn())
            .await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires a PostgreSQL test database"]
    async fn creates_and_drops_sample_table() -> crate::Result<()> {
        let db = TestDB::new(&super::test_config()).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let exists: bool = sqlx::query_scalar("SELECT to_regclass('pg_temp.sample') IS NOT NULL")
            .fetch_one(conn.handle().conn())
            .await?;
        assert!(exists);

        teardown(&mut conn.handle()).await?;
        let exists: bool = sqlx::query_scalar("SELECT to_regclass('pg_temp.sample') IS NOT NULL")
            .fetch_one(conn.handle().conn())
            .await?;
        assert!(!exists);
        Ok(())
    }

    pub mod entity {
        use chrono::Utc;

        #[derive(Debug, PartialEq, sqlx::FromRow)]
        pub struct Sample {
            pub id: i64,
            pub name: String,
            pub created_at: chrono::DateTime<Utc>,
            pub updated_at: chrono::DateTime<Utc>,
        }
    }

    pub mod repository {
        use crate::sqlxdb::tests::TestHandle;
        use crate::sqlxdb::tests::sample::entity;

        pub async fn find_by_id(
            handle: &mut TestHandle<'_>,
            id: i64,
        ) -> crate::Result<Option<entity::Sample>> {
            Ok(sqlx::query_as::<_, entity::Sample>(
                "SELECT id, name, created_at, updated_at FROM sample WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(handle.conn())
            .await?)
        }

        pub async fn insert(
            handle: &mut TestHandle<'_>,
            record: &entity::Sample,
        ) -> crate::Result<()> {
            sqlx::query(
                "INSERT INTO sample (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4)",
            )
            .bind(record.id)
            .bind(&record.name)
            .bind(record.created_at)
            .bind(record.updated_at)
            .execute(handle.conn())
            .await?;
            Ok(())
        }

        pub async fn update(
            handle: &mut TestHandle<'_>,
            record: &entity::Sample,
        ) -> crate::Result<()> {
            sqlx::query("UPDATE sample SET name = $2, updated_at = $3 WHERE id = $1")
                .bind(record.id)
                .bind(&record.name)
                .bind(record.updated_at)
                .execute(handle.conn())
                .await?;
            Ok(())
        }

        pub async fn delete(handle: &mut TestHandle<'_>, id: i64) -> crate::Result<()> {
            sqlx::query("DELETE FROM sample WHERE id = $1")
                .bind(id)
                .execute(handle.conn())
                .await?;
            Ok(())
        }
    }

    fn sample() -> entity::Sample {
        use chrono::TimeZone as _;

        let time = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        entity::Sample {
            id: 1,
            name: "sample".into(),
            created_at: time,
            updated_at: time,
        }
    }

    #[tokio::test]
    #[ignore = "requires a PostgreSQL test database"]
    async fn insert_and_find_sample() -> crate::Result<()> {
        let db = TestDB::new(&super::test_config()).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let sample = sample();

        repository::insert(&mut conn.handle(), &sample).await?;

        let stored = repository::find_by_id(&mut conn.handle(), sample.id).await?;
        assert_eq!(stored, Some(sample));
        teardown(&mut conn.handle()).await
    }

    #[tokio::test]
    #[ignore = "requires a PostgreSQL test database"]
    async fn update_sample() -> crate::Result<()> {
        let db = TestDB::new(&super::test_config()).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let mut sample = sample();
        repository::insert(&mut conn.handle(), &sample).await?;
        sample.name = "updated".into();
        sample.updated_at += chrono::Duration::seconds(1);

        repository::update(&mut conn.handle(), &sample).await?;

        let stored = repository::find_by_id(&mut conn.handle(), sample.id).await?;
        assert_eq!(stored, Some(sample));
        teardown(&mut conn.handle()).await
    }

    #[tokio::test]
    #[ignore = "requires a PostgreSQL test database"]
    async fn delete_sample() -> crate::Result<()> {
        let db = TestDB::new(&super::test_config()).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let sample = sample();
        repository::insert(&mut conn.handle(), &sample).await?;

        repository::delete(&mut conn.handle(), sample.id).await?;

        let stored = repository::find_by_id(&mut conn.handle(), sample.id).await?;
        assert_eq!(stored, None);
        teardown(&mut conn.handle()).await
    }

    #[tokio::test]
    #[ignore = "requires a PostgreSQL test database"]
    async fn transaction_commit_and_rollback() -> crate::Result<()> {
        let db = TestDB::new(&super::test_config()).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let mut sample = sample();

        // Insert and commit, then verify the record remains visible.
        let mut tx = conn.begin().await?;
        repository::insert(&mut tx.handle(), &sample).await?;
        tx.commit().await?;

        let stored = repository::find_by_id(&mut conn.handle(), sample.id).await?;
        assert_eq!(stored.as_ref(), Some(&sample));

        // Update and roll back, then verify the committed record is unchanged.
        let mut tx = conn.begin().await?;
        sample.name = "rolled back".into();
        repository::update(&mut tx.handle(), &sample).await?;
        let changed = repository::find_by_id(&mut tx.handle(), sample.id).await?;
        assert_eq!(changed.as_ref(), Some(&sample));
        tx.rollback().await?;

        let after_rollback = repository::find_by_id(&mut conn.handle(), sample.id).await?;
        assert_eq!(after_rollback, stored);
        teardown(&mut conn.handle()).await
    }
}

fn test_config() -> Config {
    Config {
        url: std::env::var("ESPR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://tlab:tlab@192.168.1.20:15432/tlab".to_string()),
        max_connections: 10,
    }
}

#[tokio::test]
#[ignore = "requires a PostgreSQL test database"]
async fn connects_to_database() -> crate::Result<()> {
    let db = TestDB::new(&test_config()).await?;
    let mut conn = db.conn().await?;
    let value: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(conn.handle().conn())
        .await?;
    assert_eq!(value, 1);
    Ok(())
}
