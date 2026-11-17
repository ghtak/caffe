use super::*;

fn test_config() -> Config {
    Config {
        username: std::env::var("ESPR_TEST_ORACLE_USERNAME").unwrap_or_else(|_| "tlab".into()),
        password: std::env::var("ESPR_TEST_ORACLE_PASSWORD").unwrap_or_else(|_| "Tlab_123".into()),
        connect_string: std::env::var("ESPR_TEST_ORACLE_CONNECT_STRING")
            .unwrap_or_else(|_| "192.168.1.20:11521/FREEPDB1".into()),
        max_connections: 10,
    }
}

#[tokio::test]
async fn rejects_zero_pool_size() {
    let mut config = test_config();
    config.max_connections = 0;
    assert!(matches!(
        Database::new(&config).await,
        Err(crate::Error::OracleDatabaseError(_))
    ));
}

#[tokio::test]
#[ignore = "requires an Oracle test database"]
async fn connects_to_database() -> Result<()> {
    let db = Database::new(&test_config()).await?;
    let value = db
        .interact(|pool| {
            let conn = pool.acquire()?;
            let row = conn.query_row("SELECT 1 FROM DUAL", &[])?;
            Ok(row.get::<i64>(0)?)
        })
        .await?;
    assert_eq!(value, 1);
    Ok(())
}

mod sample {
    use oracledb::{Connection, OracleTimestamp};

    use super::*;

    fn setup(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE PRIVATE TEMPORARY TABLE ORA$PTT_SAMPLE (
                id NUMBER(19),
                name VARCHAR2(255),
                created_at TIMESTAMP,
                updated_at TIMESTAMP
            ) ON COMMIT PRESERVE DEFINITION",
            &[],
        )?;
        Ok(())
    }

    fn teardown(conn: &Connection) -> Result<()> {
        conn.execute("DROP TABLE ORA$PTT_SAMPLE", &[])?;
        Ok(())
    }

    mod entity {
        #[derive(Debug, PartialEq)]
        pub struct Sample {
            pub id: i64,
            pub name: String,
            pub created_at: jiff::Timestamp,
            pub updated_at: jiff::Timestamp,
        }
    }

    mod repository {
        use super::entity::Sample;
        use super::{Connection, OracleTimestamp};
        use crate::Result;

        fn from_oracle(value: OracleTimestamp) -> anyhow::Result<jiff::Timestamp> {
            let datetime = jiff::civil::DateTime::new(
                value.year(),
                value.month().try_into()?,
                value.day().try_into()?,
                value.hour().try_into()?,
                value.minute().try_into()?,
                value.second().try_into()?,
                value.nanoseconds().try_into()?,
            )?;
            Ok(jiff::tz::offset(0).to_timestamp(datetime)?)
        }

        fn to_oracle(value: jiff::Timestamp) -> OracleTimestamp {
            let datetime = jiff::tz::offset(0).to_datetime(value);
            OracleTimestamp::new_timestamp(
                datetime.year(),
                datetime.month() as u8,
                datetime.day() as u8,
                datetime.hour() as u8,
                datetime.minute() as u8,
                datetime.second() as u8,
                datetime.subsec_nanosecond() as u32,
            )
        }

        #[test]
        fn oracle_timestamp_round_trips_utc_with_nanoseconds() {
            let timestamp: jiff::Timestamp = "2024-01-01T12:34:56.123456789Z".parse().unwrap();
            let oracle = to_oracle(timestamp);

            // 시간대 없는 Oracle 값은 UTC 구성 요소로 읽는다.
            assert_eq!(oracle.hour(), 12);
            assert_eq!(oracle.nanoseconds(), 123_456_789);
            assert_eq!(from_oracle(oracle).unwrap(), timestamp);
        }

        pub fn find_by_id(conn: &Connection, id: i64) -> Result<Option<Sample>> {
            let mut rows = conn.query(
                "SELECT id, name, created_at, updated_at FROM ORA$PTT_SAMPLE WHERE id = :1",
                &[&id],
            )?;
            let Some(row) = rows.next() else {
                return Ok(None);
            };
            let row = row?;
            let created: OracleTimestamp = row.get(2)?;
            let updated: OracleTimestamp = row.get(3)?;
            Ok(Some(Sample {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: from_oracle(created)?,
                updated_at: from_oracle(updated)?,
            }))
        }

        pub fn insert(conn: &Connection, record: &Sample) -> Result<()> {
            let created = to_oracle(record.created_at);
            let updated = to_oracle(record.updated_at);
            conn.execute(
                "INSERT INTO ORA$PTT_SAMPLE (id, name, created_at, updated_at) VALUES (:1, :2, :3, :4)",
                &[&record.id, &record.name, &created, &updated],
            )?;
            Ok(())
        }

        pub fn update(conn: &Connection, record: &Sample) -> Result<()> {
            let updated = to_oracle(record.updated_at);
            conn.execute(
                "UPDATE ORA$PTT_SAMPLE SET name = :1, updated_at = :2 WHERE id = :3",
                &[&record.name, &updated, &record.id],
            )?;
            Ok(())
        }

        pub fn delete(conn: &Connection, id: i64) -> Result<()> {
            conn.execute("DELETE FROM ORA$PTT_SAMPLE WHERE id = :1", &[&id])?;
            Ok(())
        }
    }

    fn sample() -> entity::Sample {
        let time = jiff::Timestamp::from_second(1_704_067_200).unwrap();
        entity::Sample {
            id: 1,
            name: "sample".into(),
            created_at: time,
            updated_at: time,
        }
    }

    #[tokio::test]
    #[ignore = "requires an Oracle test database"]
    async fn creates_and_drops_sample_table() -> Result<()> {
        let db = Database::new(&super::test_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let row = conn.query_row(
                "SELECT COUNT(*) FROM USER_PRIVATE_TEMP_TABLES WHERE TABLE_NAME = 'ORA$PTT_SAMPLE'",
                &[],
            )?;
            assert_eq!(row.get::<i64>(0)?, 1);
            teardown(&conn)?;
            let row = conn.query_row(
                "SELECT COUNT(*) FROM USER_PRIVATE_TEMP_TABLES WHERE TABLE_NAME = 'ORA$PTT_SAMPLE'",
                &[],
            )?;
            assert_eq!(row.get::<i64>(0)?, 0);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    #[ignore = "requires an Oracle test database"]
    async fn insert_and_find_sample() -> Result<()> {
        let db = Database::new(&super::test_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let sample = sample();

            repository::insert(&conn, &sample)?;

            let stored = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(stored, Some(sample));
            teardown(&conn)
        })
        .await
    }

    #[tokio::test]
    #[ignore = "requires an Oracle test database"]
    async fn update_sample() -> Result<()> {
        let db = Database::new(&super::test_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let mut sample = sample();
            repository::insert(&conn, &sample)?;
            sample.name = "updated".into();
            sample.updated_at += jiff::SignedDuration::from_secs(1);

            repository::update(&conn, &sample)?;

            let stored = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(stored, Some(sample));
            teardown(&conn)
        })
        .await
    }

    #[tokio::test]
    #[ignore = "requires an Oracle test database"]
    async fn delete_sample() -> Result<()> {
        let db = Database::new(&super::test_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let sample = sample();
            repository::insert(&conn, &sample)?;

            repository::delete(&conn, sample.id)?;

            let stored = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(stored, None);
            teardown(&conn)
        })
        .await
    }

    #[tokio::test]
    #[ignore = "requires an Oracle test database"]
    async fn transaction_commit_and_rollback() -> Result<()> {
        let db = Database::new(&super::test_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let mut sample = sample();

            // Insert and commit, then verify the record remains visible.
            repository::insert(&conn, &sample)?;
            conn.commit()?;
            let stored = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(stored.as_ref(), Some(&sample));

            // Update and roll back, then verify the committed record is
            // unchanged.
            sample.name = "rolled back".into();
            repository::update(&conn, &sample)?;
            let changed = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(changed.as_ref(), Some(&sample));
            conn.rollback()?;
            let after_rollback = repository::find_by_id(&conn, sample.id)?;
            assert_eq!(after_rollback, stored);
            teardown(&conn)
        })
        .await
    }
}
