use super::*;
use crate::app::files::entity::FileStatus;
use crate::app::files::file_group_attach as usecase;

impl Fixture {
    async fn attach(
        &self,
        group: Uuid,
        caller: Option<i64>,
        body: Value,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let mut request = Request::post(format!("/api/file-groups/{group}/attach"))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(id) = caller {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.token(id)?));
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::from(body.to_string()))?)
            .await?;
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        if response.status() == StatusCode::UNAUTHORIZED {
            assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        }
        Ok((
            response.status(),
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?,
        ))
    }

    async fn stored(&self, id: Uuid) -> anyhow::Result<crate::app::files::entity::File> {
        Ok(
            repository::find_by_id(&mut self.database.conn().await?.handle(), id)
                .await?
                .unwrap(),
        )
    }
}

fn body(attach: &[Uuid], detach: &[Uuid]) -> Value {
    json!({"resource_type":"post", "resource_key":"42", "attach_file_ids":attach, "detach_file_ids":detach})
}

fn command(
    fixture: &Fixture,
    group_id: Uuid,
    attach: Vec<Uuid>,
    detach: Vec<Uuid>,
) -> usecase::Command {
    usecase::Command {
        caller: UserClaims::new(fixture.database.user_id, UserStatus::Active, vec![]),
        group_id,
        resource_type: "post".into(),
        resource_key: "42".into(),
        attach_file_ids: attach,
        detach_file_ids: detach,
    }
}

mod contract {
    use super::*;

    #[tokio::test]
    async fn attaches_selected_files_in_input_order_and_preserves_unselected_metadata()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let unselected = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let before = fixture.stored(unselected).await?;
        let first = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let second = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let (status, output) = fixture
            .attach(
                group,
                Some(fixture.database.user_id),
                body(&[second, first], &[]),
            )
            .await?;
        let file = fixture.stored(first).await?;
        // 선택 파일만 연결하고 입력 순서·미선택 메타데이터를 유지한다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            output,
            json!({"code":"OK", "data":{"group_id":group, "attached_file_ids":[second, first], "detached_file_ids":[]}})
        );
        assert_eq!(file.status, FileStatus::Attached);
        assert_eq!(file.resource_type.as_deref(), Some("post"));
        assert_eq!(file.resource_key.as_deref(), Some("42"));
        assert_eq!(fixture.stored(unselected).await?, before);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn combined_change_detaches_own_file_without_policy_and_keeps_other_attachments()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let pending = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let attached = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let untouched = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let before = fixture.stored(untouched).await?;
        let (status, _) = fixture
            .attach(
                group,
                Some(fixture.database.user_id),
                body(&[pending], &[attached]),
            )
            .await?;
        let deleted = fixture.stored(attached).await?;
        // 본인 해제는 정책 조회 없이 연결과 함께 확정하고 다른 첨부는 유지한다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(fixture.stored(pending).await?.status, FileStatus::Attached);
        assert_eq!(deleted.status, FileStatus::Deleted);
        assert_eq!(deleted.deleted_at, Some(deleted.updated_at));
        assert_eq!(fixture.stored(untouched).await?, before);
        assert!(fixture.policy.calls.lock().unwrap().is_empty());
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn bound_deleted_history_overrides_earlier_unbound_pending_for_subsequent_upload()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let unbound = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        *fixture.policy.decision.lock().unwrap() = Ok(true);
        let (status, uploaded) = fixture.upload(Some(group), "a").await?;
        let file = fixture.stored(uuid(&uploaded, "file_id")).await?;
        // NULL 선행 행보다 삭제 이력의 귀속을 우선 상속한다.
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(file.resource_type.as_deref(), Some("post"));
        assert_eq!(file.resource_key.as_deref(), Some("42"));
        assert!(fixture.stored(unbound).await?.resource_type.is_none());
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn repeated_detach_is_authorized_noop_with_original_timestamps() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        let before = fixture.stored(id).await?;
        let (status, output) = fixture
            .attach(group, Some(fixture.database.user_id), body(&[], &[id]))
            .await?;
        // 삭제된 본인 파일도 응답에 포함하되 행과 S3를 변경하지 않는다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(output["data"]["detached_file_ids"], json!([id]));
        assert_eq!(fixture.stored(id).await?, before);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn empty_request_does_not_bind_unbound_group_and_detach_defaults_to_empty()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let before = fixture.stored(id).await?;
        let mut input = body(&[], &[]);
        input.as_object_mut().unwrap().remove("detach_file_ids");
        let (status, output) = fixture
            .attach(group, Some(fixture.database.user_id), input)
            .await?;
        // 빈 요청은 발급 이력만 확인하고 미귀속 상태를 보존한다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(output["data"]["detached_file_ids"], json!([]));
        assert_eq!(fixture.stored(id).await?, before);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn foreign_attachment_requires_edit_and_can_be_detached_by_editor() -> anyhow::Result<()>
    {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        *fixture.policy.decision.lock().unwrap() = Ok(true);
        let (status, _) = fixture.attach(group, Some(-1), body(&[], &[id])).await?;
        // 타인 업로드 파일은 원본 리소스의 Edit 정책으로 해제한다.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(fixture.stored(id).await?.status, FileStatus::Deleted);
        assert_eq!(*fixture.policy.calls.lock().unwrap(), vec![Action::Edit]);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn internal_changes_share_callers_commit_with_content() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let mut connection = fixture.database.conn().await?;
        sqlx::query("CREATE TABLE test_content (title text NOT NULL)")
            .execute(connection.handle().conn())
            .await?;
        let mut tx = connection.begin().await?;
        sqlx::query("INSERT INTO test_content VALUES ('content')")
            .execute(tx.handle().conn())
            .await?;
        usecase::execute_in_tx(
            &mut tx.handle(),
            fixture.policy.as_ref(),
            3600,
            command(&fixture, group, vec![id], vec![]),
        )
        .await?;
        let mut observer = fixture.database.conn().await?;
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM test_content")
            .fetch_one(observer.handle().conn())
            .await?;
        // 내부 진입점은 커밋하지 않으며 호출자 커밋으로 콘텐츠와 파일이 함께
        // 노출된다.
        assert_eq!(before, 0);
        assert_eq!(fixture.stored(id).await?.status, FileStatus::Pending);
        tx.commit().await?;
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM test_content")
            .fetch_one(observer.handle().conn())
            .await?;
        assert_eq!(after, 1);
        assert_eq!(fixture.stored(id).await?.status, FileStatus::Attached);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        drop(observer);
        drop(connection);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn internal_changes_and_content_disappear_on_caller_rollback() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let before = fixture.stored(id).await?;
        let mut connection = fixture.database.conn().await?;
        sqlx::query("CREATE TABLE test_content (title text NOT NULL)")
            .execute(connection.handle().conn())
            .await?;
        let mut tx = connection.begin().await?;
        sqlx::query("INSERT INTO test_content VALUES ('content')")
            .execute(tx.handle().conn())
            .await?;
        usecase::execute_in_tx(
            &mut tx.handle(),
            fixture.policy.as_ref(),
            3600,
            command(&fixture, group, vec![id], vec![]),
        )
        .await?;
        let failure = sqlx::query("INSERT INTO test_content VALUES (NULL)")
            .execute(tx.handle().conn())
            .await;
        tx.rollback().await?;
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM test_content")
            .fetch_one(connection.handle().conn())
            .await?;
        // 파일 연결 후 콘텐츠 저장 실패도 호출자 롤백으로 모두 취소한다.
        assert!(failure.is_err());
        assert_eq!(count, 0);
        assert_eq!(fixture.stored(id).await?, before);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        drop(connection);
        fixture.cleanup().await
    }
}

mod edge {
    use super::*;

    #[tokio::test]
    async fn rejects_blank_binding_duplicates_and_intersecting_arrays_without_changes()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let before = fixture.stored(id).await?;
        let mut blank_type = body(&[id], &[]);
        blank_type["resource_type"] = json!(" ");
        let mut blank_key = body(&[id], &[]);
        blank_key["resource_key"] = json!(" ");
        for input in [
            blank_type,
            blank_key,
            body(&[id, id], &[]),
            body(&[], &[id, id]),
            body(&[id], &[id]),
        ] {
            let (status, output) = fixture
                .attach(group, Some(fixture.database.user_id), input)
                .await?;
            // 요청 의미 오류는 전체 요청을 거절한다.
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(output["code"], "INVALID_REQUEST");
            assert_eq!(fixture.stored(id).await?, before);
        }
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn rejects_unknown_group_unknown_file_and_foreign_group_file() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let foreign = fixture
            .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
            .await?;
        for (target_group, input, expected) in [
            (Uuid::new_v4(), body(&[], &[]), StatusCode::NOT_FOUND),
            (
                group,
                body(&[id, Uuid::new_v4()], &[]),
                StatusCode::NOT_FOUND,
            ),
            (group, body(&[id, foreign], &[]), StatusCode::BAD_REQUEST),
        ] {
            let (status, _) = fixture
                .attach(target_group, Some(fixture.database.user_id), input)
                .await?;
            // 존재하지 않는 ID와 타 그룹 ID를 구분하며 정상 파일도 변경하지
            // 않는다.
            assert_eq!(status, expected);
            assert_eq!(fixture.stored(id).await?.status, FileStatus::Pending);
        }
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn foreign_pending_cannot_attach_even_with_edit_permission() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        *fixture.policy.decision.lock().unwrap() = Ok(true);
        let (status, _) = fixture.attach(group, Some(-1), body(&[id], &[])).await?;
        // 편집권으로 다른 업로더의 pending 연결을 우회하지 않는다.
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(fixture.stored(id).await?.status, FileStatus::Pending);
        assert!(fixture.policy.calls.lock().unwrap().is_empty());
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn rejects_attached_deleted_attach_and_pending_detach() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let pending = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let attached = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let deleted = fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        for input in [
            body(&[pending, attached], &[]),
            body(&[pending, deleted], &[]),
            body(&[], &[pending]),
        ] {
            let (status, _) = fixture
                .attach(group, Some(fixture.database.user_id), input)
                .await?;
            // 허용되지 않은 상태 전이를 전체 거절한다.
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(fixture.stored(pending).await?.status, FileStatus::Pending);
            assert_eq!(fixture.stored(attached).await?.status, FileStatus::Attached);
            assert_eq!(fixture.stored(deleted).await?.status, FileStatus::Deleted);
        }
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn expired_pending_is_rejected_without_cleanup() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let mut connection = fixture.database.conn().await?;
        sqlx::query("UPDATE conbira_files SET created_at=CURRENT_TIMESTAMP - INTERVAL '3600 seconds' WHERE file_id=$1")
            .bind(id).execute(connection.handle().conn()).await?;
        let before = fixture.stored(id).await?;
        let (status, _) = fixture
            .attach(group, Some(fixture.database.user_id), body(&[id], &[]))
            .await?;
        // TTL에 도달한 파일은 연결·삭제·S3 정리 없이 거절한다.
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(fixture.stored(id).await?, before);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        drop(connection);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn deleted_binding_cannot_be_reassigned_even_by_empty_request() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        let before = fixture.stored(id).await?;
        let mut input = body(&[], &[]);
        input["resource_key"] = json!("another");
        let (status, _) = fixture
            .attach(group, Some(fixture.database.user_id), input)
            .await?;
        // 삭제 이력도 최초 귀속을 유지한다.
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(fixture.stored(id).await?, before);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn corrupt_group_bindings_return_internal_without_selecting_one() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, Some("another"))
            .await?;
        let before = fixture.stored(id).await?;
        let (status, output) = fixture
            .attach(group, Some(fixture.database.user_id), body(&[id], &[]))
            .await?;
        // 기존 이력의 복수 귀속은 요청 오류가 아닌 저장 불변식 오류다.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(output["code"], "INTERNAL");
        assert_eq!(fixture.stored(id).await?, before);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn foreign_detach_denial_or_policy_outage_preserves_simultaneous_attach()
    -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let attached = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let mut connection = fixture.database.conn().await?;
        let other: i64 = sqlx::query_scalar(
            "INSERT INTO conbira_auth_users (nickname) VALUES ('other') RETURNING id",
        )
        .fetch_one(connection.handle().conn())
        .await?;
        let pending = fixture.seed(group, "pending", other, None).await?;
        for (decision, expected) in [
            (Ok(false), StatusCode::FORBIDDEN),
            (Err(()), StatusCode::SERVICE_UNAVAILABLE),
        ] {
            *fixture.policy.decision.lock().unwrap() = decision;
            let (status, _) = fixture
                .attach(group, Some(other), body(&[pending], &[attached]))
                .await?;
            // 해제 인가 실패 시 함께 요청한 연결도 저장하지 않는다.
            assert_eq!(status, expected);
            assert_eq!(fixture.stored(pending).await?.status, FileStatus::Pending);
            assert_eq!(fixture.stored(attached).await?.status, FileStatus::Attached);
        }
        drop(connection);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn deleted_foreign_file_still_requires_permission() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "deleted", fixture.database.user_id, Some("post"))
            .await?;
        let (status, _) = fixture.attach(group, Some(-1), body(&[], &[id])).await?;
        // 삭제된 파일 재요청도 인가를 우회하지 않는다.
        assert_eq!(status, StatusCode::FORBIDDEN);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn attach_requires_bearer_authentication() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let (status, output) = fixture.attach(Uuid::new_v4(), None, body(&[], &[])).await?;
        // 실제 라우터가 그룹 조회 전 인증과 공통 오류 응답을 적용한다.
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(output["code"], "UNAUTHORIZED");
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn attach_replay_is_rejected_after_commit() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let id = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        fixture
            .attach(group, Some(fixture.database.user_id), body(&[id], &[]))
            .await?;
        let before = fixture.stored(id).await?;
        let (status, _) = fixture
            .attach(group, Some(fixture.database.user_id), body(&[id], &[]))
            .await?;
        // 커밋된 연결 요청의 재전송은 상태 오류이며 기존 연결을 변경하지
        // 않는다.
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(fixture.stored(id).await?, before);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn commit_failure_rolls_back_both_updates_without_s3_requests() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let pending = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let attached = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let mut connection = fixture.database.conn().await?;
        sqlx::raw_sql("CREATE FUNCTION reject_attach_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$;
            CREATE CONSTRAINT TRIGGER reject_attach_commit AFTER UPDATE ON conbira_files DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_attach_commit();")
            .execute(connection.handle().conn()).await?;
        let (status, output) = fixture
            .attach(
                group,
                Some(fixture.database.user_id),
                body(&[pending], &[attached]),
            )
            .await?;
        // 커밋 실패는 두 변경을 취소하고 S3 호출 없이 INTERNAL을 반환한다.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(output["code"], "INTERNAL");
        assert_eq!(fixture.stored(pending).await?.status, FileStatus::Pending);
        assert_eq!(fixture.stored(attached).await?.status, FileStatus::Attached);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        drop(connection);
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn update_shortfall_rolls_back_prior_attachment() -> anyhow::Result<()> {
        let fixture = Fixture::new(8).await?;
        let group = Uuid::new_v4();
        let pending = fixture
            .seed(group, "pending", fixture.database.user_id, None)
            .await?;
        let attached = fixture
            .seed(group, "attached", fixture.database.user_id, Some("post"))
            .await?;
        let mut connection = fixture.database.conn().await?;
        sqlx::raw_sql("CREATE FUNCTION skip_detach() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='deleted' THEN RETURN NULL; END IF; RETURN NEW; END $$;
            CREATE TRIGGER skip_detach BEFORE UPDATE ON conbira_files FOR EACH ROW EXECUTE FUNCTION skip_detach();")
            .execute(connection.handle().conn()).await?;
        let (status, output) = fixture
            .attach(
                group,
                Some(fixture.database.user_id),
                body(&[pending], &[attached]),
            )
            .await?;
        // 저장 건수 부족 시 앞서 수행한 연결도 롤백한다.
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(output["code"], "BUSY");
        assert_eq!(fixture.stored(pending).await?.status, FileStatus::Pending);
        assert_eq!(fixture.stored(attached).await?.status, FileStatus::Attached);
        drop(connection);
        fixture.cleanup().await
    }
}
