mod contract {
    use super::super::{Database, Handle};

    type TestDB = Database<sqlx::Postgres>;
    type TestHandle<'c> = Handle<'c, sqlx::Postgres>;

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

    mod entity {
        #[derive(Debug, PartialEq, sqlx::FromRow)]
        pub struct Sample {
            pub id: i64,
            pub name: String,
            #[sqlx(try_from = "jiff_sqlx::Timestamp")]
            pub created_at: jiff::Timestamp,
            #[sqlx(try_from = "jiff_sqlx::Timestamp")]
            pub updated_at: jiff::Timestamp,
        }
    }

    mod repository {
        use super::{TestHandle, entity};

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
            .bind(jiff_sqlx::Timestamp::from(record.created_at))
            .bind(jiff_sqlx::Timestamp::from(record.updated_at))
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
                .bind(jiff_sqlx::Timestamp::from(record.updated_at))
                .execute(handle.conn())
                .await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn transaction_commit_and_rollback() -> crate::Result<()> {
        let db = TestDB::new(&crate::test_support::database_config(10)).await?;
        let mut conn = db.conn().await?;
        setup(&mut conn.handle()).await?;
        let timestamp = jiff::Timestamp::from_second(1_704_067_200).unwrap();
        let mut sample = entity::Sample {
            id: 1,
            name: "sample".into(),
            created_at: timestamp,
            updated_at: timestamp,
        };

        // Insert and commit, then verify the record remains visible.
        let mut tx = conn.begin().await?;
        repository::insert(&mut tx.handle(), &sample).await?;
        tx.commit().await?;

        let stored = repository::find_by_id(&mut conn.handle(), sample.id).await?;

        // The committed record is visible outside the transaction.
        assert_eq!(stored.as_ref(), Some(&sample));

        // Update and roll back, then verify the committed record is unchanged.
        let mut tx = conn.begin().await?;
        sample.name = "rolled back".into();
        repository::update(&mut tx.handle(), &sample).await?;
        let changed = repository::find_by_id(&mut tx.handle(), sample.id).await?;

        // The update is visible within the transaction before rollback.
        assert_eq!(changed.as_ref(), Some(&sample));
        tx.rollback().await?;

        let after_rollback = repository::find_by_id(&mut conn.handle(), sample.id).await?;

        // Rolling back restores the previously committed record.
        assert_eq!(after_rollback, stored);
        teardown(&mut conn.handle()).await
    }

    #[tokio::test]
    async fn connects_to_database() -> crate::Result<()> {
        let db = TestDB::new(&crate::test_support::database_config(10)).await?;
        let mut conn = db.conn().await?;
        let value: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(conn.handle().conn())
            .await?;
        // A pooled connection can execute a query and return its value.
        assert_eq!(value, 1);
        Ok(())
    }
}
