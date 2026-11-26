mod contract {
    use std::pin::Pin;

    use axum::http::{Method, StatusCode};

    use crate::app::files::s3::{
        ObjectHead, PresignGet, PutObject, S3ObjectAdapter, S3ObjectPort, StorageError,
    };
    use crate::app::files::test_support::{MockS3, config};

    fn put(mock: &MockS3, key: &str, bytes: &[u8], max: u64) -> PutObject {
        PutObject {
            location: mock.location(key),
            content_type: "text/plain".into(),
            body: Box::pin(std::io::Cursor::new(bytes.to_vec()))
                as Pin<Box<dyn tokio::io::AsyncRead + Send>>,
            max_bytes: max,
        }
    }

    #[tokio::test]
    async fn initialization_does_not_contact_s3() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let _adapter = mock.adapter().await?;
        let _regional = S3ObjectAdapter::new(&config(None)).await?;
        // SDK 생성은 지역 endpoint와 custom endpoint 모두 원격 요청을 하지
        // 않는다.
        assert!(mock.state.lock().unwrap().requests.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn stores_exact_bytes_and_downloads_through_presigned_url() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        let stored = adapter
            .put(put(&mock, "prefix/file-id", b"hello", 5))
            .await
            .unwrap();
        let head = adapter.head(&stored.location).await.unwrap();
        let before = mock.state.lock().unwrap().requests.len();
        let now = jiff::Timestamp::now();
        let signed = adapter
            .presign_get(PresignGet {
                location: stored.location,
                original_filename: "보고서\r\n\".txt".into(),
                ttl_seconds: 300,
                now,
            })
            .await
            .unwrap();
        // 서명은 HEAD 등 원격 요청을 추가하지 않고 TTL·안전 인코딩을 포함한다.
        assert_eq!(stored.size_bytes, 5);
        assert!(
            matches!(head, ObjectHead::Present { size_bytes: 5, content_type } if content_type == "text/plain")
        );
        assert_eq!(before, mock.state.lock().unwrap().requests.len());
        assert_eq!(
            signed.expires_at,
            now.checked_add(jiff::SignedDuration::from_secs(300))?
        );
        let parsed = url::Url::parse(&signed.url)?;
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query.get("X-Amz-Expires").map(String::as_str), Some("300"));
        assert!(query.contains_key("X-Amz-Signature"));
        assert!(query["response-content-disposition"].contains("%0D%0A%22"));
        let response = openidconnect::reqwest::Client::new()
            .get(signed.url)
            .send()
            .await?;
        assert!(response.status().is_success());
        assert_eq!(response.bytes().await?.as_ref(), b"hello");
        Ok(())
    }

    #[tokio::test]
    async fn oversized_stream_is_rejected_without_putting_object() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        let result = adapter.put(put(&mock, "oversized", b"hello!", 5)).await;
        // 상한+1에서 중단하며 완료된 객체와 PUT 요청이 없다.
        assert!(matches!(result, Err(StorageError::FileTooLarge)));
        let state = mock.state.lock().unwrap();
        assert!(state.objects.is_empty());
        assert!(state.requests.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn conditional_put_refuses_to_overwrite_existing_object() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        adapter
            .put(put(&mock, "existing", b"first", 10))
            .await
            .unwrap();
        let result = adapter.put(put(&mock, "existing", b"second", 10)).await;
        // If-None-Match 조건으로 원본 객체를 보존한다.
        assert!(matches!(result, Err(StorageError::InvalidObjectRequest)));
        assert_eq!(
            mock.state.lock().unwrap().objects["/conbira-test/existing"]
                .0
                .as_ref(),
            b"first"
        );
        Ok(())
    }

    #[tokio::test]
    async fn head_distinguishes_absence_permission_failure_and_outage() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        {
            let mut state = mock.state.lock().unwrap();
            state.failures.insert(
                (Method::HEAD, "/conbira-test/denied".into()),
                StatusCode::FORBIDDEN,
            );
            state.failures.insert(
                (Method::HEAD, "/conbira-test/unavailable".into()),
                StatusCode::SERVICE_UNAVAILABLE,
            );
        }
        // 없음·권한 거부·일시 장애를 서로 다른 결과로 반환한다.
        assert!(matches!(
            adapter.head(&mock.location("missing")).await,
            Ok(ObjectHead::Missing)
        ));
        assert!(matches!(
            adapter.head(&mock.location("denied")).await,
            Err(StorageError::StorageConfiguration(_))
        ));
        assert!(matches!(
            adapter.head(&mock.location("unavailable")).await,
            Err(StorageError::StorageUnavailable(_))
        ));
        Ok(())
    }

    #[tokio::test]
    async fn delete_is_idempotent_and_never_retries_failures() -> anyhow::Result<()> {
        let mock = MockS3::new().await?;
        let adapter = mock.adapter().await?;
        adapter
            .put(put(&mock, "delete", b"hello", 5))
            .await
            .unwrap();
        adapter.delete(&mock.location("delete")).await.unwrap();
        adapter.delete(&mock.location("delete")).await.unwrap();
        mock.state.lock().unwrap().failures.insert(
            (Method::DELETE, "/conbira-test/failure".into()),
            StatusCode::SERVICE_UNAVAILABLE,
        );
        let error = adapter.delete(&mock.location("failure")).await;
        // 미존재 재삭제는 성공하고 장애는 요청 1회로 종료한다.
        assert!(matches!(error, Err(StorageError::StorageUnavailable(_))));
        let state = mock.state.lock().unwrap();
        assert!(state.objects.is_empty());
        assert_eq!(
            state
                .requests
                .iter()
                .filter(|(method, path)| *method == Method::DELETE && path.ends_with("failure"))
                .count(),
            1
        );
        Ok(())
    }
}
