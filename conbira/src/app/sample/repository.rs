use crate::app::container::db::Handle;
use crate::app::sample::entity::Sample;
use crate::{Error, Result};

#[allow(dead_code)]
pub async fn find_by_id(handle: &mut Handle<'_>, id: i64) -> Result<Option<Sample>> {
    sqlx::query_as::<_, Sample>(
        "SELECT id, name, created_at, updated_at FROM conbira_sample WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(handle.conn())
    .await
    .map_err(|error| {
        Error::internal("Failed to find sample")
            .at("sample.repository.find_by_id")
            .cause(error)
    })
}

#[allow(dead_code)]
pub async fn insert(handle: &mut Handle<'_>, sample: &Sample) -> Result<()> {
    sqlx::query(
        "INSERT INTO conbira_sample (id, name, created_at, updated_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(sample.id)
    .bind(&sample.name)
    .bind(jiff_sqlx::Timestamp::from(sample.created_at))
    .bind(jiff_sqlx::Timestamp::from(sample.updated_at))
    .execute(handle.conn())
    .await
    .map_err(|error| {
        Error::internal("Failed to insert sample")
            .at("sample.repository.insert")
            .cause(error)
    })?;
    Ok(())
}

#[allow(dead_code)]
pub async fn update(handle: &mut Handle<'_>, sample: &Sample) -> Result<()> {
    sqlx::query("UPDATE conbira_sample SET name = $2, updated_at = $3 WHERE id = $1")
        .bind(sample.id)
        .bind(&sample.name)
        .bind(jiff_sqlx::Timestamp::from(sample.updated_at))
        .execute(handle.conn())
        .await
        .map_err(|error| {
            Error::internal("Failed to update sample")
                .at("sample.repository.update")
                .cause(error)
        })?;
    Ok(())
}

#[allow(dead_code)]
pub async fn delete(handle: &mut Handle<'_>, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM conbira_sample WHERE id = $1")
        .bind(id)
        .execute(handle.conn())
        .await
        .map_err(|error| {
            Error::internal("Failed to delete sample")
                .at("sample.repository.delete")
                .cause(error)
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::container::db::Database;

    async fn setup() -> anyhow::Result<espr::sqlxdb::Conn<sqlx::Postgres>> {
        let database = Database::new(&espr::sqlxdb::Config {
            url: std::env::var("CONBIRA_TEST_DATABASE_URL")?,
            max_connections: 1,
        })
        .await?;
        let mut connection = database.conn().await?;
        sqlx::query(
            "CREATE TEMPORARY TABLE conbira_sample (
                id BIGINT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(connection.handle().conn())
        .await?;
        Ok(connection)
    }

    #[tokio::test]
    #[ignore = "requires CONBIRA_TEST_DATABASE_URL"]
    async fn inserts_reads_updates_and_deletes_sample() -> anyhow::Result<()> {
        let mut connection = setup().await?;
        let mut handle = connection.handle();
        let created_at = jiff::Timestamp::from_second(1_700_000_000).unwrap();
        let mut sample = Sample {
            id: 42,
            name: "first".into(),
            created_at,
            updated_at: created_at,
        };

        // 존재하지 않는 ID는 None을 반환한다.
        assert_eq!(find_by_id(&mut handle, sample.id).await?, None);

        // 삽입 후 모든 필드가 저장한 값과 동일하다.
        insert(&mut handle, &sample).await?;
        assert_eq!(
            find_by_id(&mut handle, sample.id).await?,
            Some(Sample {
                id: sample.id,
                name: sample.name.clone(),
                created_at: sample.created_at,
                updated_at: sample.updated_at,
            })
        );

        // 같은 ID의 중복 삽입은 실패하고 기존 데이터는 유지된다.
        assert!(insert(&mut handle, &sample).await.is_err());
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
        sample.updated_at = created_at + jiff::SignedDuration::from_secs(60);
        update(&mut handle, &sample).await?;
        assert_eq!(find_by_id(&mut handle, sample.id).await?, Some(sample));

        // 삭제 후 해당 ID는 더 이상 조회되지 않는다.
        delete(&mut handle, 42).await?;
        assert_eq!(find_by_id(&mut handle, 42).await?, None);
        Ok(())
    }
}
