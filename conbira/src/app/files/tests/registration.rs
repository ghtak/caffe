mod edge {
    use axum::http::{Method, StatusCode};
    use uuid::Uuid;

    use crate::app::files::repository;
    use crate::app::files::s3::{PutObject, S3ObjectPort, StorageError};
    use crate::app::files::test_support::{MockS3, TestDatabase};

    async fn registration_failure(fail_delete: bool) -> anyhow::Result<()> {
        let fixture = TestDatabase::new().await?;
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        let mut command = fixture.file(Uuid::new_v4());
        command.uploaded_by = -1; // 유지한 사용자 FK로 DB 등록만 실패시킨다.
        let location = mock.location(&command.s3_object_key);
        adapter
            .put(PutObject {
                location: location.clone(),
                content_type: "text/plain".into(),
                body: Box::pin(std::io::Cursor::new(b"hello")),
                max_bytes: 5,
            })
            .await
            .unwrap();
        let mut connection = fixture.conn().await?;
        let mut tx = connection.begin().await?;
        assert!(
            repository::insert(&mut tx.handle(), &command)
                .await
                .is_err()
        );
        tx.rollback().await?;
        let object_path = format!("/conbira-test/{}", command.s3_object_key);
        if fail_delete {
            mock.state.lock().unwrap().failures.insert(
                (Method::DELETE, object_path.clone()),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
        // 저장소 실패를 확정한 후 한 번만 보상한다. HTTP 조합은 API 테스트에서
        // 검증한다.
        let result = adapter.delete(&location).await;
        if fail_delete {
            assert!(matches!(result, Err(StorageError::StorageUnavailable(_))));
        } else {
            result.unwrap();
        }
        {
            let state = mock.state.lock().unwrap();
            assert_eq!(
                state
                    .requests
                    .iter()
                    .filter(|(method, _)| *method == Method::DELETE)
                    .count(),
                1
            );
            assert_eq!(state.objects.contains_key(&object_path), fail_delete);
        }
        fixture.cleanup().await
    }

    #[tokio::test]
    async fn confirmed_registration_failure_can_be_compensated_once() -> anyhow::Result<()> {
        registration_failure(false).await
    }

    #[tokio::test]
    async fn failed_compensation_does_not_issue_additional_delete_requests() -> anyhow::Result<()> {
        registration_failure(true).await
    }
}
