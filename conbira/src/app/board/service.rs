#[derive(serde::Deserialize)]
pub struct AttachmentChanges {
    pub attach_file_ids: Vec<uuid::Uuid>,
    pub detach_file_ids: Vec<uuid::Uuid>,
}

pub(super) async fn change_attachments(
    tx: &mut crate::app::db::Handle<'_>,
    service: &crate::app::file_asset::service::FileAssetService,
    caller_id: i64,
    post: &super::entity::Post,
    changes: AttachmentChanges,
) -> crate::Result<()> {
    use crate::app::file_asset::service::FileAssetStatus;
    use crate::{Code, Error};
    if post.board_slug != "data" {
        return Err(Error::new(
            Code::InvalidRequest,
            "Attachments are not allowed on this board",
        )
        .at(crate::here!()));
    }
    for id in changes.attach_file_ids {
        if !service.attach_owned(tx, id, caller_id).await? {
            return Err(Error::new(
                Code::InvalidRequest,
                "File asset is not an owned pending upload",
            )
            .at(crate::here!()));
        }
        super::repository::post_attachment::insert(tx, post.id, id).await?;
    }
    for id in changes.detach_file_ids {
        if !super::repository::post_attachment::remove(tx, post.id, id).await?
            || !service
                .change_status(tx, id, FileAssetStatus::Attached, FileAssetStatus::Deleted)
                .await?
        {
            return Err(
                Error::new(Code::InvalidRequest, "Post attachment cannot be removed")
                    .at(crate::here!()),
            );
        }
    }
    Ok(())
}
