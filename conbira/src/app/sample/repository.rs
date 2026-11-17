use crate::Result;
use crate::app::container::db::Handle;
use crate::app::sample::entity::Sample;
use crate::error::ResultExt as _;

pub struct NewSample {
    pub name: String,
}

pub async fn insert(handle: &mut Handle<'_>, sample: &NewSample) -> Result<Sample> {
    sqlx::query_as::<_, Sample>(
        "INSERT INTO conbira_sample (name) VALUES ($1) RETURNING id, name, created_at, updated_at",
    )
    .bind(&sample.name)
    .fetch_one(handle.conn())
    .await
    .internal("Failed to insert sample", "sample.repository.insert")
}

#[allow(dead_code)]
pub async fn find_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<Sample>> {
    sqlx::query_as::<_, Sample>(
        "SELECT id, name, created_at, updated_at FROM conbira_sample WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .internal("Failed to find sample", "sample.repository.find_by_id")
}

#[allow(dead_code)]
pub async fn update(handle: &mut Handle<'_>, sample: &Sample) -> Result<()> {
    sqlx::query(
        "UPDATE conbira_sample SET name = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
    )
    .bind(sample.id)
    .bind(&sample.name)
    .execute(handle.conn())
    .await
    .internal("Failed to update sample", "sample.repository.update")?;
    Ok(())
}

#[allow(dead_code)]
pub async fn delete(handle: &mut Handle<'_>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM conbira_sample WHERE id = $1")
        .bind(id)
        .execute(handle.conn())
        .await
        .internal("Failed to delete sample", "sample.repository.delete")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::container::db::Database;

    async fn setup() -> anyhow::Result<espr::sqlxdb::Conn<sqlx::Postgres>> {
        let database = Database::new(&espr::sqlxdb::Config {
            url: std::env::var("CONBIRA_TEST_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://tlab:tlab@192.168.1.20:15432/tlab".to_string()),
            max_connections: 1,
        })
        .await?;
        let mut connection = database.conn().await?;
        let schema = include_str!("../../../migrations/0001_sample.sql")
            .replace("CREATE TABLE IF NOT EXISTS", "CREATE TEMPORARY TABLE");
        sqlx::query(sqlx::AssertSqlSafe(schema))
            .execute(connection.handle().conn())
            .await?;
        Ok(connection)
    }

    #[tokio::test]
    #[ignore = "requires CONBIRA_TEST_DATABASE_URL"]
    async fn creates_sample_with_generated_id_and_server_timestamps() -> anyhow::Result<()> {
        let mut connection = setup().await?;
        let mut handle = connection.handle();
        let input = NewSample {
            name: "created sample".into(),
        };
        let created = insert(&mut handle, &input).await?;
        let sample = sqlx::query_as::<_, Sample>(
            "SELECT id, name, created_at, updated_at FROM conbira_sample WHERE name = $1",
        )
        .bind("created sample")
        .fetch_one(handle.conn())
        .await?;

        // ID가 생성되고 두 시각은 생성 요청 시점의 같은 서버 시각이다.
        assert_eq!(created, sample);
        assert!(sample.id > 0);
        assert_eq!(sample.name, "created sample");
        assert_eq!(sample.created_at, sample.updated_at);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "requires CONBIRA_TEST_DATABASE_URL"]
    async fn inserts_reads_updates_and_deletes_sample() -> anyhow::Result<()> {
        let mut connection = setup().await?;
        let mut handle = connection.handle();
        let input = NewSample {
            name: "first".into(),
        };
        let mut sample = insert(&mut handle, &input).await?;

        // 삽입 후 모든 필드가 저장한 값과 동일하다.
        assert_eq!(
            find_by_id(&mut handle, sample.id).await?,
            Some(Sample {
                id: sample.id,
                name: sample.name.clone(),
                created_at: sample.created_at,
                updated_at: sample.updated_at,
            })
        );

        // 수정한 이름과 수정 시각이 반영되고 생성 시각은 유지된다.
        sample.name = "updated".into();
        let old_timestamp = jiff::Timestamp::from_second(1_700_000_000).unwrap();
        sqlx::query("UPDATE conbira_sample SET updated_at = $2 WHERE id = $1")
            .bind(sample.id)
            .bind(jiff_sqlx::Timestamp::from(old_timestamp))
            .execute(handle.conn())
            .await?;
        sample.updated_at = old_timestamp;
        update(&mut handle, &sample).await?;
        let id = sample.id;
        let updated = find_by_id(&mut handle, id).await?.unwrap();
        assert_eq!(updated.name, "updated");
        assert_eq!(updated.created_at, sample.created_at);
        assert!(updated.updated_at > old_timestamp);

        // 삭제 후 해당 ID는 더 이상 조회되지 않는다.
        delete(&mut handle, id).await?;
        assert_eq!(find_by_id(&mut handle, id).await?, None);
        Ok(())
    }
}
