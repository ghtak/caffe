mod edge {
    use super::super::*;

    #[tokio::test]
    async fn rejects_zero_pool_size() {
        let mut config = crate::test_support::oracle_config();
        config.max_connections = 0;
        let result = Database::new(&config).await;

        // The Oracle pool rejects a zero connection limit.
        assert!(matches!(result, Err(crate::Error::OracleDatabaseError(_))));
    }
}

mod contract {
    use oracledb::{Connection, OracleTimestamp};

    use super::super::*;

    #[tokio::test]
    #[ignore = "requires a running Oracle database"]
    async fn connects_to_database() -> Result<()> {
        let db = Database::new(&crate::test_support::oracle_config()).await?;
        let value = db
            .interact(|pool| {
                let conn = pool.acquire()?;
                let row = conn.query_row("SELECT 1 FROM DUAL", &[])?;
                Ok(row.get::<i64>(0)?)
            })
            .await?;
        // A pooled Oracle connection can execute a query and return its value.
        assert_eq!(value, 1);
        Ok(())
    }

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

        pub(super) fn from_oracle(value: OracleTimestamp) -> anyhow::Result<jiff::Timestamp> {
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

        pub(super) fn to_oracle(value: jiff::Timestamp) -> OracleTimestamp {
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
    }

    #[test]
    fn oracle_timestamp_round_trips_utc_with_nanoseconds() {
        let timestamp: jiff::Timestamp = "2024-01-01T12:34:56.123456789Z".parse().unwrap();
        let oracle = repository::to_oracle(timestamp);

        // 시간대 없는 Oracle 값은 UTC 구성 요소로 읽는다.
        assert_eq!(oracle.hour(), 12);
        assert_eq!(oracle.nanoseconds(), 123_456_789);
        assert_eq!(repository::from_oracle(oracle).unwrap(), timestamp);
    }

    #[tokio::test]
    #[ignore = "requires a running Oracle database"]
    async fn transaction_commit_and_rollback() -> Result<()> {
        let db = Database::new(&crate::test_support::oracle_config()).await?;
        db.interact(|pool| {
            let conn = pool.acquire()?;
            setup(&conn)?;
            let timestamp = jiff::Timestamp::from_second(1_704_067_200).unwrap();
            let mut sample = entity::Sample {
                id: 1,
                name: "sample".into(),
                created_at: timestamp,
                updated_at: timestamp,
            };

            // Insert and commit, then verify the record remains visible.
            repository::insert(&conn, &sample)?;
            conn.commit()?;
            let stored = repository::find_by_id(&conn, sample.id)?;

            // The committed record remains visible on the connection.
            assert_eq!(stored.as_ref(), Some(&sample));

            // Update and roll back, then verify the committed record is
            // unchanged.
            sample.name = "rolled back".into();
            repository::update(&conn, &sample)?;
            let changed = repository::find_by_id(&conn, sample.id)?;

            // The pending update is visible before rollback.
            assert_eq!(changed.as_ref(), Some(&sample));
            conn.rollback()?;
            let after_rollback = repository::find_by_id(&conn, sample.id)?;

            // Rollback restores the previously committed record.
            assert_eq!(after_rollback, stored);
            teardown(&conn)
        })
        .await
    }
}
