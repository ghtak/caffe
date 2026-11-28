use uuid::Uuid;

use crate::app::container::db::Handle;
use crate::error::ResultExt as _;

pub async fn list(
    handle: &mut Handle<'_>,
    post_id: i64,
) -> crate::Result<Vec<crate::app::file_asset::FileAssetSummary>> {
    sqlx::query_as("SELECT f.id, f.status, f.uploaded_by, f.original_filename, f.size_bytes, f.content_type, f.created_at, f.updated_at FROM conbira_post_attachments a JOIN conbira_file_assets f ON f.id = a.file_asset_id WHERE a.post_id = $1 AND f.status = 'attached' ORDER BY a.created_at, a.file_asset_id")
        .bind(post_id).fetch_all(handle.conn()).await.map_err_internal("Failed to list post attachments", crate::here!())
}

pub async fn insert(handle: &mut Handle<'_>, post_id: i64, asset_id: Uuid) -> crate::Result<()> {
    sqlx::query("INSERT INTO conbira_post_attachments (post_id, file_asset_id) VALUES ($1, $2)")
        .bind(post_id)
        .bind(asset_id)
        .execute(handle.conn())
        .await
        .map_err_internal("Failed to attach asset to post", crate::here!())?;
    Ok(())
}

pub async fn remove(handle: &mut Handle<'_>, post_id: i64, asset_id: Uuid) -> crate::Result<bool> {
    let result = sqlx::query(
        "DELETE FROM conbira_post_attachments WHERE post_id = $1 AND file_asset_id = $2",
    )
    .bind(post_id)
    .bind(asset_id)
    .execute(handle.conn())
    .await
    .map_err_internal("Failed to detach post asset", crate::here!())?;
    Ok(result.rows_affected() == 1)
}

pub async fn is_downloadable(
    handle: &mut Handle<'_>,
    post_id: i64,
    asset_id: Uuid,
) -> crate::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM conbira_post_attachments a JOIN conbira_posts p ON p.id = a.post_id JOIN conbira_file_assets f ON f.id = a.file_asset_id WHERE a.post_id = $1 AND a.file_asset_id = $2 AND p.deleted_at IS NULL AND f.status = 'attached')")
        .bind(post_id).bind(asset_id).fetch_one(handle.conn()).await
        .map_err_internal("Failed to find post attachment", crate::here!())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[tokio::test]
    async fn lists_only_attached_assets_for_the_requested_post() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(2, |context| async move {
            let mut conn = context.db.conn().await?;
            sqlx::raw_sql(include_str!("../../../../migrations/0020_posts.sql"))
                .execute(conn.handle().conn()).await?;
            let user: i64 = sqlx::query_scalar("INSERT INTO conbira_auth_users (nickname) VALUES ('writer') RETURNING id")
                .fetch_one(conn.handle().conn()).await?;
            let post: i64 = sqlx::query_scalar("INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('data', $1, 'post') RETURNING id")
                .bind(user).fetch_one(conn.handle().conn()).await?;
            let other_post: i64 = sqlx::query_scalar("INSERT INTO conbira_posts (board_slug, author_id, title) VALUES ('data', $1, 'other') RETURNING id")
                .bind(user).fetch_one(conn.handle().conn()).await?;
            let attached = Uuid::new_v4();
            for (id, status, post_id) in [(attached, "attached", post), (Uuid::new_v4(), "pending", post), (Uuid::new_v4(), "deleted", post), (Uuid::new_v4(), "attached", other_post)] {
                sqlx::query("INSERT INTO conbira_file_assets (id, status, uploaded_by, s3_bucket, s3_object_key, original_filename, size_bytes, content_type) VALUES ($1, $2, $3, 'bucket', $1::text, 'report.txt', 4, 'text/plain')")
                    .bind(id).bind(status).bind(user).execute(conn.handle().conn()).await?;
                super::insert(&mut conn.handle(), post_id, id).await?;
            }
            let assets = super::list(&mut conn.handle(), post).await?;
            assert_eq!(assets.len(), 1);
            assert_eq!(assets[0].id, attached);
            assert_eq!(assets[0].original_filename, "report.txt");
            Ok(())
        }).await
    }
}
