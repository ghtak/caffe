pub mod endpoint {
    use std::sync::Arc;

    use axum::extract::State;
    use uuid::Uuid;

    use crate::app::Container;
    use crate::app::auth::claims::UserClaims;
    use crate::app::http::extract::{ApiJson, ApiPath};
    use crate::app::http::response::ApiResult;

    #[derive(serde::Deserialize, espr_derive::Validate)]
    pub struct RequestBody {
        pub resource_type: String,
        pub resource_key: String,
        pub attach_file_ids: Vec<Uuid>,
        #[serde(default)]
        pub detach_file_ids: Vec<Uuid>,
    }

    pub async fn handler(
        State(container): State<Arc<Container>>,
        caller: UserClaims,
        ApiPath(group_id): ApiPath<Uuid>,
        ApiJson(body): ApiJson<RequestBody>,
    ) -> ApiResult<super::usecase::Output> {
        let output = super::usecase::execute(
            &container.db,
            container.file_resource_access.as_ref(),
            container.config.files.pending_ttl_seconds,
            super::usecase::Command {
                caller,
                group_id,
                resource_type: body.resource_type,
                resource_key: body.resource_key,
                attach_file_ids: body.attach_file_ids,
                detach_file_ids: body.detach_file_ids,
            },
        )
        .await?;
        Ok(output.into())
    }
}

pub mod usecase {
    use std::collections::HashSet;

    use uuid::Uuid;

    use crate::app::DatabaseExt as _;
    use crate::app::auth::claims::UserClaims;
    use crate::app::container::db::{Database, Handle};
    use crate::app::files::access::{self, Action, ResourceAccessPort};
    use crate::app::files::entity::FileStatus;
    use crate::app::files::repository;
    use crate::error::ResultExt as _;
    use crate::{Code, Error, Result};

    pub struct Command {
        pub caller: UserClaims,
        pub group_id: Uuid,
        pub resource_type: String,
        pub resource_key: String,
        pub attach_file_ids: Vec<Uuid>,
        pub detach_file_ids: Vec<Uuid>,
    }

    #[derive(Debug, serde::Serialize)]
    pub struct Output {
        pub group_id: Uuid,
        pub attached_file_ids: Vec<Uuid>,
        pub detached_file_ids: Vec<Uuid>,
    }

    pub async fn execute(
        db: &Database,
        policy: &dyn ResourceAccessPort,
        pending_ttl_seconds: u64,
        command: Command,
    ) -> Result<Output> {
        let mut tx = db.begin_tx(crate::here!()).await?;
        let output = execute_in_tx(&mut tx.handle(), policy, pending_ttl_seconds, command).await?;
        tx.commit()
            .await
            .map_err_internal("File transaction commit failed", crate::here!())?;
        Ok(output)
    }

    /// Requires an active caller-owned transaction. The caller must roll back
    /// the entire transaction on any error. This function never commits or
    /// calls S3.
    pub async fn execute_in_tx(
        tx: &mut Handle<'_>,
        policy: &dyn ResourceAccessPort,
        pending_ttl_seconds: u64,
        command: Command,
    ) -> Result<Output> {
        let mut ids = HashSet::new();
        if command.resource_type.trim().is_empty()
            || command.resource_key.trim().is_empty()
            || !command
                .attach_file_ids
                .iter()
                .chain(&command.detach_file_ids)
                .all(|id| ids.insert(*id))
        {
            return Err(
                Error::new(Code::InvalidRequest, "Invalid file attachment request")
                    .at(crate::here!()),
            );
        }
        let now = jiff::Timestamp::now();
        let group = repository::find_group(tx, command.group_id)
            .await?
            .ok_or_else(|| Error::new(Code::NotFound, "File group not found").at(crate::here!()))?;
        if group
            .resource_type
            .as_ref()
            .is_some_and(|kind| kind != &command.resource_type)
            || group
                .resource_key
                .as_ref()
                .is_some_and(|key| key != &command.resource_key)
        {
            return Err(
                Error::new(Code::InvalidRequest, "File group binding conflict").at(crate::here!()),
            );
        }
        let mut detach = Vec::new();
        for (file_ids, attaching) in [
            (&command.attach_file_ids, true),
            (&command.detach_file_ids, false),
        ] {
            for file_id in file_ids {
                let file = repository::find_by_id(tx, *file_id).await?.ok_or_else(|| {
                    Error::new(Code::NotFound, "File not found").at(crate::here!())
                })?;
                if file.group_id != command.group_id {
                    return Err(
                        Error::new(Code::InvalidRequest, "File group mismatch").at(crate::here!())
                    );
                }
                if attaching {
                    if file.status != FileStatus::Pending {
                        return Err(Error::new(
                            Code::InvalidRequest,
                            "Only pending files can be attached",
                        )
                        .at(crate::here!()));
                    }
                    if !access::file(policy, &command.caller, &file, Action::Edit).await? {
                        return Err(
                            Error::new(Code::Forbidden, "File access denied").at(crate::here!())
                        );
                    }
                    let age = now.duration_since(file.created_at);
                    if age.as_secs() >= 0 && age.as_secs() as u64 >= pending_ttl_seconds {
                        return Err(Error::new(Code::InvalidRequest, "Pending file expired")
                            .at(crate::here!()));
                    }
                } else {
                    if file.status == FileStatus::Pending {
                        return Err(Error::new(
                            Code::InvalidRequest,
                            "Pending files cannot be detached",
                        )
                        .at(crate::here!()));
                    }
                    if file.uploaded_by != command.caller.id
                        && !access::bound(
                            policy,
                            &command.caller,
                            file.resource_type.as_deref(),
                            file.resource_key.as_deref(),
                            Action::Edit,
                        )
                        .await?
                    {
                        return Err(
                            Error::new(Code::Forbidden, "File access denied").at(crate::here!())
                        );
                    }
                    if file.status == FileStatus::Attached {
                        detach.push(*file_id);
                    }
                }
            }
        }
        if !command.attach_file_ids.is_empty()
            && repository::mark_attached(
                tx,
                command.group_id,
                &command.attach_file_ids,
                &command.resource_type,
                &command.resource_key,
                now,
            )
            .await?
                != command.attach_file_ids.len() as u64
        {
            return Err(Error::new(Code::Busy, "File state changed").at(crate::here!()));
        }
        if !detach.is_empty()
            && repository::mark_detached(
                tx,
                command.group_id,
                &detach,
                &command.resource_type,
                &command.resource_key,
                now,
            )
            .await?
                != detach.len() as u64
        {
            return Err(Error::new(Code::Busy, "File state changed").at(crate::here!()));
        }
        Ok(Output {
            group_id: command.group_id,
            attached_file_ids: command.attach_file_ids,
            detached_file_ids: command.detach_file_ids,
        })
    }
}
