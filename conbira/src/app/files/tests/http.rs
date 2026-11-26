use std::sync::{Arc, Mutex};

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use uuid::Uuid;

use crate::app::Container;
use crate::app::auth::claims::UserClaims;
use crate::app::auth::entity::UserStatus;
use crate::app::files::access::{Action, ResourceAccessPort};
use crate::app::files::repository;
use crate::test_support::files::{FileFixture, MockS3};
use crate::{Code, Error};

struct Policy {
    decision: Mutex<Result<bool, ()>>,
    calls: Mutex<Vec<Action>>,
    revoke_after_first_check: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl ResourceAccessPort for Policy {
    async fn decide(
        &self,
        _: &UserClaims,
        kind: &str,
        _: &str,
        action: Action,
    ) -> crate::Result<bool> {
        {
            let mut calls = self.calls.lock().unwrap();
            calls.push(action);
            if calls.len() > 1
                && self
                    .revoke_after_first_check
                    .load(std::sync::atomic::Ordering::Relaxed)
            {
                return Ok(false);
            }
        }
        if kind != "post" {
            return Ok(false);
        }
        self.decision
            .lock()
            .unwrap()
            .map_err(|_| Error::new(Code::Busy, "Resource policy unavailable"))
    }
}

struct Fixture {
    database: FileFixture,
    container: Arc<Container>,
    mock: MockS3,
    policy: Arc<Policy>,
    router: axum::Router,
}

impl Fixture {
    async fn new(
        context: Arc<crate::test_support::TestContext>,
        max_file_size_bytes: u64,
    ) -> anyhow::Result<Self> {
        let mut container = context.container().await?;
        let database = FileFixture::new(context).await?;
        let mock = MockS3::new().await?;
        let policy = Arc::new(Policy {
            decision: Mutex::new(Ok(false)),
            calls: Mutex::new(vec![]),
            revoke_after_first_check: std::sync::atomic::AtomicBool::new(false),
        });
        container.file_storage = Arc::new(mock.adapter().await?);
        container.file_resource_access = policy.clone();
        container.config.files = crate::test_support::files::config(Some(mock.endpoint.clone()));
        container.config.files.max_file_size_bytes = max_file_size_bytes;
        let container = Arc::new(container);
        let router = crate::app::http::router(container.clone())?;
        Ok(Self {
            database,
            container,
            mock,
            policy,
            router,
        })
    }

    fn token(&self, id: i64) -> anyhow::Result<String> {
        Ok(self
            .container
            .jwt_token_service
            .issue(
                &id.to_string(),
                UserClaims::new(id, UserStatus::Active, vec![]).try_into()?,
            )?
            .token)
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        caller: Option<i64>,
        multipart: Option<String>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(id) = caller {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", self.token(id)?));
        }
        let body = match multipart {
            Some(body) => {
                builder = builder.header(
                    header::CONTENT_TYPE,
                    "multipart/form-data; boundary=test-boundary",
                );
                Body::from(body)
            }
            None => Body::empty(),
        };
        let response = self.router.clone().oneshot(builder.body(body)?).await?;
        // 모든 파일 응답은 캐시 금지. 401은 Bearer challenge 포함.
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        if response.status() == StatusCode::UNAUTHORIZED {
            assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
        }
        Ok((
            response.status(),
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?,
        ))
    }

    async fn upload(
        &self,
        group: Option<Uuid>,
        bytes: &str,
    ) -> anyhow::Result<(StatusCode, Value)> {
        self.request(
            Method::POST,
            "/api/files",
            Some(self.database.user_id),
            Some(multipart(bytes, group)),
        )
        .await
    }

    async fn seed(
        &self,
        group: Uuid,
        status: &str,
        owner: i64,
        binding: Option<&str>,
    ) -> anyhow::Result<Uuid> {
        let mut connection = self.database.db.conn().await?;
        let mut command = self.database.file(group);
        command.uploaded_by = owner;
        repository::insert(&mut connection.handle(), &command).await?;
        sqlx::query("UPDATE conbira_files SET status=$2, resource_type=$3, resource_key=$4, deleted_at=CASE WHEN $2='deleted' THEN CURRENT_TIMESTAMP ELSE NULL END WHERE file_id=$1")
            .bind(command.file_id).bind(status).bind(binding).bind(binding.map(|_| "42"))
            .execute(connection.handle().conn()).await?;
        Ok(command.file_id)
    }
}

fn part(name: &str, value: &str) -> String {
    format!("--test-boundary\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
}
fn file_part(bytes: &str) -> String {
    format!(
        "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"report.txt\"\r\nContent-Type: application/x-uncommon\r\n\r\n{bytes}\r\n"
    )
}
fn multipart(bytes: &str, group: Option<Uuid>) -> String {
    let mut body = file_part(bytes);
    if let Some(group) = group {
        body += &part("group_id", &group.to_string());
    }
    body + "--test-boundary--\r\n"
}
fn uuid(body: &Value, key: &str) -> Uuid { body["data"][0][key].as_str().unwrap().parse().unwrap() }

#[path = "file_group_attach.rs"]
mod attach_tests;

mod contract {
    use super::*;

    #[tokio::test]
    async fn upload_list_download_delete_through_app_router() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, uploaded) = fixture.upload(None, "12345678").await?;
            // 파일 바이트 상한을 수용하고 공개 DTO에서 저장 위치를 제외한다.
            assert_eq!(status, StatusCode::CREATED);
            let file_id = uuid(&uploaded, "file_id");
            let group_id = uuid(&uploaded, "group_id");
            assert_eq!(uploaded["data"][0]["status"], "pending");
            assert_eq!(uploaded["data"][0]["size_bytes"], 8);
            assert_eq!(uploaded["data"][0].as_object().unwrap().len(), 7);
            let mut connection = fixture.database.db.conn().await?;
            let stored = repository::find_by_id(&mut connection.handle(), file_id)
                .await?
                .unwrap();
            assert_eq!(stored.uploaded_by, fixture.database.user_id);
            let object_path = format!("/conbira-test/test-files/{file_id}");
            assert_eq!(
                fixture.mock.state.lock().unwrap().objects[&object_path]
                    .0
                    .as_ref(),
                b"12345678"
            );
            let (status, listed) = fixture
                .request(
                    Method::GET,
                    &format!("/api/file-groups/{group_id}/files"),
                    Some(fixture.database.user_id),
                    None,
                )
                .await?;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(listed["data"], uploaded["data"]);
            let (status, download) = fixture
                .request(
                    Method::GET,
                    &format!("/api/files/{file_id}/download"),
                    Some(fixture.database.user_id),
                    None,
                )
                .await?;
            assert_eq!(status, StatusCode::OK);
            let expires: jiff::Timestamp =
                download["data"]["expires_at"].as_str().unwrap().parse()?;
            assert!(expires > jiff::Timestamp::now());
            let url = download["data"]["url"].as_str().unwrap();
            assert!(url.contains("X-Amz-Expires=300"));
            assert!(url.contains("response-content-disposition="));
            // 서명 URL로 실제 객체를 읽을 수 있다.
            let bytes = tokio::process::Command::new("curl")
                .args(["-fsS", "--max-time", "5", url])
                .output()
                .await?;
            assert!(bytes.status.success());
            assert_eq!(bytes.stdout, b"12345678");
            let (status, deleted) = fixture
                .request(
                    Method::DELETE,
                    &format!("/api/files/{file_id}"),
                    Some(fixture.database.user_id),
                    None,
                )
                .await?;
            assert_eq!((status, deleted), (StatusCode::OK, json!({"code":"OK"})));
            let after = repository::find_by_id(&mut connection.handle(), file_id)
                .await?
                .unwrap();
            assert_eq!(after.status, super::super::entity::FileStatus::Deleted);
            assert_eq!(after.deleted_at, Some(after.updated_at));
            assert_eq!(after.group_id, stored.group_id);
            assert_eq!(after.s3_object_key, stored.s3_object_key);
            assert!(
                !fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .objects
                    .contains_key(&object_path)
            );
            assert_eq!(
                fixture
                    .request(
                        Method::DELETE,
                        &format!("/api/files/{file_id}"),
                        Some(fixture.database.user_id),
                        None
                    )
                    .await?
                    .0,
                StatusCode::OK
            );
            let replay = repository::find_by_id(&mut connection.handle(), file_id)
                .await?
                .unwrap();
            assert_eq!(after, replay);
            assert_eq!(
                fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::DELETE)
                    .count(),
                1
            );
            assert_eq!(
                fixture
                    .request(
                        Method::GET,
                        &format!("/api/files/{file_id}/download"),
                        Some(fixture.database.user_id),
                        None
                    )
                    .await?
                    .0,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                fixture
                    .request(
                        Method::GET,
                        &format!("/api/file-groups/{group_id}/files"),
                        Some(-1),
                        None
                    )
                    .await?
                    .1["data"],
                json!([])
            );
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn configured_eight_file_limit_accepts_exactly_eight() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(file_part("hello").repeat(8) + "--test-boundary--\r\n"),
                )
                .await?;
            // 설정 상한인 파일 8개를 같은 그룹에 일괄 등록한다.
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["data"].as_array().unwrap().len(), 8);
            let mut connection = fixture.database.db.conn().await?;
            let rows =
                repository::list_by_group_id(&mut connection.handle(), uuid(&body, "group_id"))
                    .await?;
            assert_eq!(rows.len(), 8);
            assert_eq!(fixture.mock.state.lock().unwrap().objects.len(), 8);
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn ninth_file_rejects_entire_request_before_put() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(file_part("hello").repeat(9) + "--test-boundary--\r\n"),
                )
                .await?;
            // 9번째 파일에서 요청 전체를 거절하며 앞선 8개도 저장하지 않는다.
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(body["code"], "INVALID_REQUEST");
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            let mut connection = fixture.database.db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
                .fetch_one(connection.handle().conn())
                .await?;
            assert_eq!(count, 0);
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn multiple_files_share_new_group_and_preserve_input_order() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(
                        file_part("12345678")
                            + &file_part("second").replace("report.txt", "second.txt")
                            + "--test-boundary--\r\n",
                    ),
                )
                .await?;
            // 파일별 상한을 적용하고 입력 순서·공통 그룹·독립 파일 ID를
            // 보존한다.
            assert_eq!(status, StatusCode::CREATED);
            let files = body["data"].as_array().unwrap();
            assert_eq!(files.len(), 2);
            assert_eq!(files[0]["group_id"], files[1]["group_id"]);
            assert_ne!(files[0]["file_id"], files[1]["file_id"]);
            assert_eq!(files[0]["original_filename"], "report.txt");
            assert_eq!(files[1]["original_filename"], "second.txt");
            let mut connection = fixture.database.db.conn().await?;
            let rows =
                repository::list_by_group_id(&mut connection.handle(), uuid(&body, "group_id"))
                    .await?;
            assert_eq!(rows.len(), 2);
            for (file, bytes) in files.iter().zip(["12345678", "second"]) {
                let path = format!(
                    "/conbira-test/test-files/{}",
                    file["file_id"].as_str().unwrap()
                );
                assert_eq!(
                    fixture.mock.state.lock().unwrap().objects[&path].0.as_ref(),
                    bytes.as_bytes()
                );
                assert_eq!(file["status"], "pending");
                assert_eq!(file["size_bytes"], bytes.len());
            }
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn multiple_files_inherit_existing_group_binding() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let group = Uuid::new_v4();
            fixture
                .seed(group, "deleted", fixture.database.user_id, Some("post"))
                .await?;
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(
                        file_part("first")
                            + &part("group_id", &group.to_string())
                            + &file_part("second")
                            + "--test-boundary--\r\n",
                    ),
                )
                .await?;
            // 두 파일 모두 현재 그룹 귀속을 상속하고 pending으로 등록한다.
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["data"].as_array().unwrap().len(), 2);
            let mut connection = fixture.database.db.conn().await?;
            let rows = repository::list_by_group_id(&mut connection.handle(), group).await?;
            assert_eq!(rows.len(), 3);
            for file in rows
                .iter()
                .filter(|file| file.status == super::super::entity::FileStatus::Pending)
            {
                assert_eq!(file.resource_type.as_deref(), Some("post"));
                assert_eq!(file.resource_key.as_deref(), Some("42"));
                assert_eq!(file.uploaded_by, fixture.database.user_id);
            }
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn later_oversized_file_rejects_entire_batch_before_put() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(file_part("first") + &file_part("123456789") + "--test-boundary--\r\n"),
                )
                .await?;
            // 뒤 파일이 상한을 넘으면 앞 파일도 저장하지 않는다.
            assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(body["code"], "INVALID_REQUEST");
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn later_put_failure_compensates_successful_files_without_metadata() -> anyhow::Result<()>
    {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            fixture.mock.state.lock().unwrap().fail_put_number = Some(2);
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(fixture.database.user_id),
                    Some(file_part("first") + &file_part("second") + "--test-boundary--\r\n"),
                )
                .await?;
            // 두 번째 PUT 실패 시 첫 번째 객체만 한 번 보상한다.
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(body["code"], "BUSY");
            {
                let state = fixture.mock.state.lock().unwrap();
                assert!(state.objects.is_empty());
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::PUT)
                        .count(),
                    2
                );
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::DELETE)
                        .count(),
                    1
                );
            }
            let mut connection = fixture.database.db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
                .fetch_one(connection.handle().conn())
                .await?;
            assert_eq!(count, 0);
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn later_insert_failure_rolls_back_batch_and_compensates_every_object()
    -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
        let fixture = Fixture::new(database, 8).await?;
        let mut connection = fixture.database.db.conn().await?;
        sqlx::query("CREATE FUNCTION reject_second_file() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.size_bytes = 6 THEN RAISE EXCEPTION 'injected second insert failure'; END IF; RETURN NEW; END $$")
            .execute(connection.handle().conn()).await?;
        sqlx::query("CREATE TRIGGER reject_second_file BEFORE INSERT ON conbira_files FOR EACH ROW EXECUTE FUNCTION reject_second_file()")
            .execute(connection.handle().conn()).await?;
        let (status, body) = fixture
            .request(
                Method::POST,
                "/api/files",
                Some(fixture.database.user_id),
                Some(file_part("first") + &file_part("second") + "--test-boundary--\r\n"),
            )
            .await?;
        // 두 번째 INSERT 실패로 첫 행도 롤백하고 성공한 두 객체를 각각 한 번
        // 삭제한다.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "INTERNAL");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
            .fetch_one(connection.handle().conn())
            .await?;
        assert_eq!(count, 0);
        {
            let state = fixture.mock.state.lock().unwrap();
            assert!(state.objects.is_empty());
            let deletes: Vec<_> = state
                .requests
                .iter()
                .filter(|(m, _)| m == Method::DELETE)
                .map(|(_, path)| path)
                .collect();
            assert_eq!(deletes.len(), 2);
            assert_ne!(deletes[0], deletes[1]);
        }
        drop(connection);
        Ok(())
        }).await
    }

    #[tokio::test]
    async fn zero_byte_uncommon_mime_is_accepted() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture.upload(None, "").await?;
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["data"][0]["size_bytes"], 0);
            assert_eq!(body["data"][0]["content_type"], "application/x-uncommon");
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn existing_deleted_group_accepts_group_field_after_file_and_inherits_binding()
    -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let group = Uuid::new_v4();
            fixture
                .seed(group, "deleted", fixture.database.user_id, Some("post"))
                .await?;
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            let (status, body) = fixture.upload(Some(group), "hello").await?;
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(uuid(&body, "group_id"), group);
            let mut connection = fixture.database.db.conn().await?;
            let file = repository::find_by_id(&mut connection.handle(), uuid(&body, "file_id"))
                .await?
                .unwrap();
            let history = repository::find_group(&mut connection.handle(), group)
                .await?
                .unwrap();
            assert_eq!(history.uploaded_by, [fixture.database.user_id]);
            let rows = repository::list_by_group_id(&mut connection.handle(), group).await?;
            assert_eq!(rows.len(), 2);
            assert!(rows.iter().any(
                |row| row.status == super::super::entity::FileStatus::Deleted
                    && row.deleted_at.is_some()
            ));
            assert_eq!(file.resource_type.as_deref(), Some("post"));
            assert_eq!(file.uploaded_by, fixture.database.user_id);
            // 귀속 pending 다운로드는 리소스 인가를 재사용하지 않는다.
            *fixture.policy.decision.lock().unwrap() = Ok(false);
            let before = fixture.policy.calls.lock().unwrap().len();
            assert_eq!(
                fixture
                    .request(
                        Method::GET,
                        &format!("/api/files/{}/download", file.file_id),
                        Some(file.uploaded_by),
                        None
                    )
                    .await?
                    .0,
                StatusCode::OK
            );
            assert_eq!(fixture.policy.calls.lock().unwrap().len(), before);
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn listing_filters_each_file_and_never_calls_s3() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let group = Uuid::new_v4();
            let own = fixture
                .seed(group, "pending", fixture.database.user_id, None)
                .await?;
            fixture
                .seed(group, "pending", fixture.database.user_id, None)
                .await?;
            let attached = fixture
                .seed(group, "attached", fixture.database.user_id, Some("post"))
                .await?;
            fixture
                .seed(
                    group,
                    "attached",
                    fixture.database.user_id,
                    Some("unsupported"),
                )
                .await?;
            fixture
                .seed(group, "deleted", fixture.database.user_id, None)
                .await?;
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            let path = format!("/api/file-groups/{group}/files");
            let (status, body) = fixture
                .request(Method::GET, &path, Some(fixture.database.user_id), None)
                .await?;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["data"].as_array().unwrap().len(), 3);
            assert_eq!(body["data"][0]["file_id"], own.to_string());
            assert_eq!(body["data"][2]["file_id"], attached.to_string());
            // 타인 pending을 숨기고 attached는 리소스 권한으로 공개한다.
            let (_, foreign) = fixture.request(Method::GET, &path, Some(-1), None).await?;
            assert_eq!(foreign["data"].as_array().unwrap().len(), 1);
            *fixture.policy.decision.lock().unwrap() = Ok(false);
            assert_eq!(
                fixture.request(Method::GET, &path, Some(-1), None).await?.1["data"],
                json!([])
            );
            *fixture.policy.decision.lock().unwrap() = Err(());
            assert_eq!(
                fixture.request(Method::GET, &path, Some(-1), None).await?.0,
                StatusCode::SERVICE_UNAVAILABLE
            );
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn attached_requires_resource_permission_even_for_uploader() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let id = fixture
                .seed(
                    Uuid::new_v4(),
                    "attached",
                    fixture.database.user_id,
                    Some("post"),
                )
                .await?;
            for (method, path) in [
                (Method::GET, format!("/api/files/{id}/download")),
                (Method::DELETE, format!("/api/files/{id}")),
            ] {
                assert_eq!(
                    fixture
                        .request(method, &path, Some(fixture.database.user_id), None)
                        .await?
                        .0,
                    StatusCode::FORBIDDEN
                );
            }
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            // 인가된 파일도 S3 객체가 없으면 URL을 발급하지 않는다.
            assert_eq!(
                fixture
                    .request(
                        Method::GET,
                        &format!("/api/files/{id}/download"),
                        Some(-1),
                        None
                    )
                    .await?
                    .0,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                fixture
                    .request(Method::DELETE, &format!("/api/files/{id}"), Some(-1), None)
                    .await?
                    .0,
                StatusCode::OK
            );
            assert!(fixture.policy.calls.lock().unwrap().contains(&Action::Read));
            assert!(fixture.policy.calls.lock().unwrap().contains(&Action::Edit));
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn foreign_pending_and_deleted_replay_require_authorization() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let pending = fixture
                .seed(
                    Uuid::new_v4(),
                    "pending",
                    fixture.database.user_id,
                    Some("post"),
                )
                .await?;
            let deleted = fixture
                .seed(
                    Uuid::new_v4(),
                    "deleted",
                    fixture.database.user_id,
                    Some("post"),
                )
                .await?;
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            for (method, path) in [
                (Method::GET, format!("/api/files/{pending}/download")),
                (Method::DELETE, format!("/api/files/{pending}")),
            ] {
                assert_eq!(
                    fixture.request(method, &path, Some(-1), None).await?.0,
                    StatusCode::FORBIDDEN
                );
            }
            *fixture.policy.decision.lock().unwrap() = Ok(false);
            assert_eq!(
                fixture
                    .request(
                        Method::DELETE,
                        &format!("/api/files/{deleted}"),
                        Some(-1),
                        None
                    )
                    .await?
                    .0,
                StatusCode::FORBIDDEN
            );
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            assert_eq!(
                fixture
                    .request(
                        Method::DELETE,
                        &format!("/api/files/{deleted}"),
                        Some(-1),
                        None
                    )
                    .await?
                    .0,
                StatusCode::OK
            );
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn all_routes_authenticate_and_map_invalid_uuid_to_common_json() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            for (method, path) in [
                (Method::POST, "/api/files"),
                (Method::GET, "/api/file-groups/invalid/files"),
                (Method::GET, "/api/files/invalid/download"),
                (Method::DELETE, "/api/files/invalid"),
            ] {
                let (status, body) = fixture.request(method.clone(), path, None, None).await?;
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(body["code"], "UNAUTHORIZED");
                if method != Method::POST {
                    let (status, body) = fixture
                        .request(method, path, Some(fixture.database.user_id), None)
                        .await?;
                    assert_eq!(status, StatusCode::BAD_REQUEST);
                    assert_eq!(body["code"], "INVALID_REQUEST");
                }
            }
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }
    async fn request(
        fixture: &Fixture,
        method: Method,
        path: &str,
        permissions: &[&str],
        body: Option<Value>,
    ) -> anyhow::Result<(StatusCode, Value)> {
        let token = fixture
            .container
            .jwt_token_service
            .issue(
                "-1",
                UserClaims::new(
                    -1,
                    UserStatus::Active,
                    permissions.iter().map(|p| (*p).into()).collect(),
                )
                .try_into()?,
            )?
            .token;
        let response = fixture
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))?,
            )
            .await?;
        Ok((
            response.status(),
            serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?,
        ))
    }

    #[tokio::test]
    async fn list_requires_explicit_list_permission() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            // 파일 관리 권한만으로 전체 목록을 조회할 수 없다.
            for permissions in [vec![], vec!["file.manage"]] {
                assert_eq!(
                    request(&fixture, Method::GET, "/api/files", &permissions, None)
                        .await?
                        .0,
                    StatusCode::FORBIDDEN
                );
            }
            assert_eq!(
                fixture
                    .request(Method::GET, "/api/files", None, None)
                    .await?
                    .0,
                StatusCode::UNAUTHORIZED
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn list_returns_non_deleted_metadata_newest_first() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let pending = fixture
                .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
                .await?;
            let attached = fixture
                .seed(
                    Uuid::new_v4(),
                    "attached",
                    fixture.database.user_id,
                    Some("post"),
                )
                .await?;
            fixture
                .seed(
                    Uuid::new_v4(),
                    "deleted",
                    fixture.database.user_id,
                    Some("post"),
                )
                .await?;
            let (status, body) = request(
                &fixture,
                Method::GET,
                "/api/files",
                &["file.list.read"],
                None,
            )
            .await?;
            // 다른 사용자의 파일도 조회하되 삭제 파일과 내부 저장소 정보는
            // 제외한다.
            assert_eq!(status, StatusCode::OK);
            let items = body["data"]["items"].as_array().unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0]["file_id"], attached.to_string());
            assert_eq!(items[1]["file_id"], pending.to_string());
            for item in items {
                assert!(item.get("s3_bucket").is_none());
                assert!(item.get("s3_object_key").is_none());
                assert!(item.get("uploaded_by").is_none());
            }
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn list_searches_literal_filename_without_case_and_paginates() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
        let fixture = Fixture::new(database, 8).await?;
        let first = fixture
            .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
            .await?;
        let second = fixture
            .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
            .await?;
        let other = fixture
            .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
            .await?;
        {
            let mut connection = fixture.database.db.conn().await?;
            sqlx::query("UPDATE conbira_files SET original_filename = CASE WHEN file_id=$1 THEN 'Report_100%.txt' WHEN file_id=$2 THEN 'report_100%.pdf' ELSE 'other.txt' END WHERE file_id=ANY($3)")
                .bind(first).bind(second).bind(vec![first, second, other]).execute(connection.handle().conn()).await?;
        }
        for (page, expected, has_next) in [
            (1, Some(second), true),
            (2, Some(first), false),
            (3, None, false),
        ] {
            let (status, body) = request(
                &fixture,
                Method::GET,
                &format!("/api/files?name=REPORT_100%25&page={page}&page_size=1"),
                &["file.list.read"],
                None,
            )
            .await?;
            // 대소문자와 관계없이 파일명 부분 검색 후 페이지 사이에서 조건을
            // 유지한다.
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["data"]["page"], page);
            assert_eq!(body["data"]["page_size"], 1);
            assert_eq!(body["data"]["has_next"], has_next);
            let items = body["data"]["items"].as_array().unwrap();
            if let Some(id) = expected {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0]["file_id"], id.to_string());
            } else {
                assert!(items.is_empty());
            }
        }
        Ok(())
        }).await
    }

    #[tokio::test]
    async fn manager_can_download_attach_and_delete_another_users_file() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (_, uploaded) = fixture.upload(None, "hello").await?;
            let id = uuid(&uploaded, "file_id");
            let group = uuid(&uploaded, "group_id");
            // 목록 권한만으로 파일 작업을 수행할 수 없다.
            assert_eq!(
                request(
                    &fixture,
                    Method::GET,
                    &format!("/api/files/{id}/download"),
                    &["file.list.read"],
                    None
                )
                .await?
                .0,
                StatusCode::FORBIDDEN
            );
            // 관리자는 리소스 정책 거절 상태에서도 타인의 대기 파일을 연결한다.
            assert_eq!(
                request(
                    &fixture,
                    Method::POST,
                    &format!("/api/file-groups/{group}/attach"),
                    &["file.manage"],
                    Some(json!({
                        "resource_type": "post", "resource_key": "42", "attach_file_ids": [id]
                    }))
                )
                .await?
                .0,
                StatusCode::OK
            );
            assert_eq!(
                request(
                    &fixture,
                    Method::GET,
                    &format!("/api/files/{id}/download"),
                    &["file.manage"],
                    None
                )
                .await?
                .0,
                StatusCode::OK
            );
            assert_eq!(
                request(
                    &fixture,
                    Method::DELETE,
                    &format!("/api/files/{id}"),
                    &["file.manage"],
                    None
                )
                .await?
                .0,
                StatusCode::OK
            );
            assert!(fixture.policy.calls.lock().unwrap().is_empty());
            Ok(())
        })
        .await
    }
}

mod edge {
    use super::*;

    #[tokio::test]
    async fn malformed_multipart_including_late_fields_never_reaches_s3() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let valid = file_part("hello");
            let cases = [
                String::new(),
                part("group_id", "invalid"),
                valid.clone() + &valid + &part("unknown", "x"),
                valid.clone() + &valid.replace("application/x-uncommon", "invalid"),
                valid.clone()
                    + &part("group_id", &Uuid::new_v4().to_string())
                    + &part("group_id", &Uuid::new_v4().to_string()),
                valid.replace("filename=\"report.txt\"", "filename=\"\""),
                valid.replace("Content-Type: application/x-uncommon\r\n", ""),
                valid.replace("application/x-uncommon", "invalid"),
            ];
            for body in cases {
                let (status, body) = fixture
                    .request(
                        Method::POST,
                        "/api/files",
                        Some(fixture.database.user_id),
                        Some(body + "--test-boundary--\r\n"),
                    )
                    .await?;
                assert_eq!(status, StatusCode::BAD_REQUEST);
                assert_eq!(body["code"], "INVALID_REQUEST");
            }
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn measured_file_bytes_over_limit_return_413_without_s3() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (status, body) = fixture.upload(None, "123456789").await?;
            assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(body["code"], "INVALID_REQUEST");
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn unknown_or_unauthorized_group_is_rejected_before_put() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let unknown = Uuid::new_v4();
            assert_eq!(
                fixture.upload(Some(unknown), "a").await?.0,
                StatusCode::NOT_FOUND
            );
            let mut connection = fixture.database.db.conn().await?;
            assert!(
                repository::find_group(&mut connection.handle(), unknown)
                    .await?
                    .is_none()
            );
            drop(connection);
            let unbound = Uuid::new_v4();
            fixture
                .seed(unbound, "deleted", fixture.database.user_id, None)
                .await?;
            assert_eq!(
                fixture
                    .request(
                        Method::POST,
                        "/api/files",
                        Some(-1),
                        Some(multipart("a", Some(unbound)))
                    )
                    .await?
                    .0,
                StatusCode::FORBIDDEN
            );
            let bound = Uuid::new_v4();
            fixture
                .seed(bound, "deleted", fixture.database.user_id, Some("post"))
                .await?;
            assert_eq!(
                fixture.upload(Some(bound), "a").await?.0,
                StatusCode::FORBIDDEN
            );
            assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn confirmed_registration_failure_compensates_once() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            // 존재하지 않는 사용자 FK로 PUT 이후 등록을 실패시킨다.
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(-1),
                    Some(multipart("hello", None)),
                )
                .await?;
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body["code"], "INTERNAL");
            {
                let state = fixture.mock.state.lock().unwrap();
                assert!(state.objects.is_empty());
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::DELETE)
                        .count(),
                    1
                );
            }
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn failed_physical_delete() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (_, uploaded) = fixture.upload(None, "hello").await?;
            let id = uuid(&uploaded, "file_id");
            fixture.mock.state.lock().unwrap().failures.insert(
                (Method::DELETE, format!("/conbira-test/test-files/{id}")),
                StatusCode::SERVICE_UNAVAILABLE,
            );
            assert_eq!(
                fixture
                    .request(
                        Method::DELETE,
                        &format!("/api/files/{id}"),
                        Some(fixture.database.user_id),
                        None
                    )
                    .await?
                    .0,
                StatusCode::OK
            );
            assert_eq!(
                fixture
                    .request(
                        Method::DELETE,
                        &format!("/api/files/{id}"),
                        Some(fixture.database.user_id),
                        None
                    )
                    .await?
                    .0,
                StatusCode::OK
            );
            assert_eq!(
                fixture
                    .request(
                        Method::GET,
                        &format!("/api/files/{id}/download"),
                        Some(fixture.database.user_id),
                        None
                    )
                    .await?
                    .0,
                StatusCode::NOT_FOUND
            );
            {
                let state = fixture.mock.state.lock().unwrap();
                assert_eq!(state.objects.len(), 1);
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::DELETE)
                        .count(),
                    1
                );
            }
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn head_errors_keep_configuration_and_outage_distinct() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (_, uploaded) = fixture.upload(None, "a").await?;
            let id = uuid(&uploaded, "file_id");
            for (injected, expected, code) in [
                (
                    StatusCode::FORBIDDEN,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INVALID_CONFIGURATION",
                ),
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    StatusCode::SERVICE_UNAVAILABLE,
                    "BUSY",
                ),
            ] {
                fixture.mock.state.lock().unwrap().failures.insert(
                    (Method::HEAD, format!("/conbira-test/test-files/{id}")),
                    injected,
                );
                let (status, body) = fixture
                    .request(
                        Method::GET,
                        &format!("/api/files/{id}/download"),
                        Some(fixture.database.user_id),
                        None,
                    )
                    .await?;
                assert_eq!(status, expected);
                assert_eq!(body["code"], code);
                assert!(!body.to_string().contains("InjectedFailure"));
            }
            Ok(())
        })
        .await
    }
    #[tokio::test]
    async fn upload_commit_failure_retains_object_without_compensation() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
        let fixture = Fixture::new(database, 8).await?;
        let mut connection = fixture.database.db.conn().await?;
        sqlx::query("ALTER TABLE conbira_files ALTER CONSTRAINT conbira_files_uploaded_by_fkey DEFERRABLE INITIALLY DEFERRED")
            .execute(connection.handle().conn()).await?;
        let (status, body) = fixture
            .request(
                Method::POST,
                "/api/files",
                Some(-1),
                Some(file_part("first") + &file_part("second") + "--test-boundary--\r\n"),
            )
            .await?;
        // COMMIT 실패는 상태 재조회나 즉시 보상 없이 객체를 남긴다.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "INTERNAL");
        {
            let state = fixture.mock.state.lock().unwrap();
            assert_eq!(state.objects.len(), 2);
            assert_eq!(
                state
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::DELETE)
                    .count(),
                0
            );
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
            .fetch_one(connection.handle().conn())
            .await?;
        assert_eq!(count, 0);
        drop(connection);
        Ok(())
        }).await
    }

    #[tokio::test]
    async fn delete_commit_failure_keeps_object_and_active_metadata() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
        let fixture = Fixture::new(database, 8).await?;
        let (_, uploaded) = fixture.upload(None, "hello").await?;
        let id = uuid(&uploaded, "file_id");
        let mut connection = fixture.database.db.conn().await?;
        sqlx::raw_sql("CREATE FUNCTION reject_file_delete() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$;
            CREATE CONSTRAINT TRIGGER reject_file_delete AFTER UPDATE ON conbira_files DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_file_delete();")
            .execute(connection.handle().conn()).await?;
        let (status, body) = fixture
            .request(
                Method::DELETE,
                &format!("/api/files/{id}"),
                Some(fixture.database.user_id),
                None,
            )
            .await?;
        // 논리 삭제 COMMIT 실패 시 물리 삭제를 호출하지 않는다.
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["code"], "INTERNAL");
        let stored = repository::find_by_id(&mut connection.handle(), id)
            .await?
            .unwrap();
        assert_eq!(stored.status, super::super::entity::FileStatus::Pending);
        assert!(stored.deleted_at.is_none());
        {
            let state = fixture.mock.state.lock().unwrap();
            assert_eq!(state.objects.len(), 1);
            assert_eq!(
                state
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::DELETE)
                    .count(),
                0
            );
        }
        drop(connection);
        Ok(())
        }).await
    }

    #[tokio::test]
    async fn failed_compensation_preserves_registration_error_and_attempts_once()
    -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            fixture
                .mock
                .state
                .lock()
                .unwrap()
                .method_failures
                .insert(Method::DELETE, StatusCode::SERVICE_UNAVAILABLE);
            let (status, body) = fixture
                .request(
                    Method::POST,
                    "/api/files",
                    Some(-1),
                    Some(multipart("hello", None)),
                )
                .await?;
            // S3 보상 장애가 원래 DB 실패 코드를 덮어쓰지 않는다.
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body["code"], "INTERNAL");
            {
                let state = fixture.mock.state.lock().unwrap();
                assert_eq!(state.objects.len(), 1);
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::DELETE)
                        .count(),
                    1
                );
            }
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn put_failure_never_inserts_metadata_or_compensates() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            for (injected, expected, code) in [
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    StatusCode::SERVICE_UNAVAILABLE,
                    "BUSY",
                ),
                (
                    StatusCode::FORBIDDEN,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INVALID_CONFIGURATION",
                ),
                (
                    StatusCode::PRECONDITION_FAILED,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL",
                ),
            ] {
                fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .method_failures
                    .insert(Method::PUT, injected);
                let (status, body) = fixture.upload(None, "hello").await?;
                assert_eq!(status, expected);
                assert_eq!(body["code"], code);
            }
            // PUT 실패는 저장된 객체 보상 경로에 진입하지 않는다.
            assert!(
                fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .all(|(m, _)| m == Method::PUT)
            );
            let mut connection = fixture.database.db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
                .fetch_one(connection.handle().conn())
                .await?;
            assert_eq!(count, 0);
            drop(connection);
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn concurrent_deletes_mark_once_and_attempt_one_physical_delete() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (_, uploaded) = fixture.upload(None, "hello").await?;
            let path = format!("/api/files/{}", uuid(&uploaded, "file_id"));
            let (first, second) = tokio::join!(
                fixture.request(Method::DELETE, &path, Some(fixture.database.user_id), None),
                fixture.request(Method::DELETE, &path, Some(fixture.database.user_id), None),
            );
            // 먼저 삭제한 요청의 커밋 후 후속 요청은 deleted를 인가하고
            // 종료한다.
            assert_eq!(first?.0, StatusCode::OK);
            assert_eq!(second?.0, StatusCode::OK);
            assert_eq!(
                fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::DELETE)
                    .count(),
                1
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn deletion_waits_for_attachment_then_checks_current_resource_permission()
    -> anyhow::Result<()> {
        use std::time::Duration;
        crate::test_support::TestContext::run(4, move |database| async move {
        let fixture = Fixture::new(database, 8).await?;
        let id = fixture
            .seed(Uuid::new_v4(), "pending", fixture.database.user_id, None)
            .await?;
        let mut writer = fixture.database.db.conn().await?;
        let mut tx = writer.begin().await?;
        sqlx::query("UPDATE conbira_files SET status='attached', resource_type='post', resource_key='42' WHERE file_id=$1")
            .bind(id).execute(tx.handle().conn()).await?;
        let path = format!("/api/files/{id}");
        let delete = fixture.request(Method::DELETE, &path, Some(fixture.database.user_id), None);
        let attach = async {
            let mut observer = fixture.database.db.conn().await?;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let waiting: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE '%conbira_files%FOR UPDATE%')")
                        .fetch_one(observer.handle().conn()).await?;
                    if waiting { break Ok::<_, anyhow::Error>(()); }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await??;
            tx.commit().await?;
            Ok::<_, anyhow::Error>(())
        };
        let (response, attached) = tokio::join!(delete, attach);
        attached?;
        // 잠금 대기 후 attached의 현재 권한을 검사하며 업로더로 우회하지
        // 않는다.
        assert_eq!(response?.0, StatusCode::FORBIDDEN);
        let stored = repository::find_by_id(&mut writer.handle(), id)
            .await?
            .unwrap();
        assert_eq!(stored.status, super::super::entity::FileStatus::Attached);
        assert!(fixture.mock.state.lock().unwrap().requests.is_empty());
        drop(writer);
        Ok(())
        }).await
    }

    #[tokio::test]
    async fn configured_file_limit_above_axum_default_is_accepted() -> anyhow::Result<()> {
        let size = 3 * 1024 * 1024;
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, size).await?;
            let (status, body) = fixture.upload(None, &"x".repeat(size as usize)).await?;
            // Axum 기본 2MiB 제한에 걸리지 않고 설정 상한의 파일을 저장한다.
            assert_eq!(status, StatusCode::CREATED);
            assert_eq!(body["data"][0]["size_bytes"], size);
            assert_eq!(
                fixture
                    .mock
                    .state
                    .lock()
                    .unwrap()
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::PUT)
                    .count(),
                1
            );
            Ok(())
        })
        .await
    }

    #[tokio::test]
    async fn independent_same_group_uploads_and_client_retries_create_new_files()
    -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let (_, first) = fixture.upload(None, "hello").await?;
            let group = uuid(&first, "group_id");
            let (second, third) = tokio::join!(
                fixture.upload(Some(group), "hello"),
                fixture.upload(Some(group), "hello")
            );
            let (second_status, second) = second?;
            let (third_status, third) = third?;
            // 같은 그룹의 독립 요청과 재시도는 각각 새 파일을 등록한다.
            assert_eq!(second_status, StatusCode::CREATED);
            assert_eq!(third_status, StatusCode::CREATED);
            assert_ne!(uuid(&second, "file_id"), uuid(&third, "file_id"));
            assert_ne!(uuid(&first, "file_id"), uuid(&second, "file_id"));
            assert_eq!(uuid(&second, "group_id"), group);
            assert_eq!(uuid(&third, "group_id"), group);
            let mut connection = fixture.database.db.conn().await?;
            let rows = repository::list_by_group_id(&mut connection.handle(), group).await?;
            assert_eq!(rows.len(), 3);
            assert!(
                rows.iter()
                    .any(|row| row.file_id == uuid(&second, "file_id"))
            );
            assert!(
                rows.iter()
                    .any(|row| row.file_id == uuid(&third, "file_id"))
            );
            drop(connection);
            Ok(())
        })
        .await
    }
    #[tokio::test]
    async fn revoked_group_permission_after_put_rolls_back_and_compensates() -> anyhow::Result<()> {
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let group = Uuid::new_v4();
            fixture
                .seed(group, "deleted", fixture.database.user_id, Some("post"))
                .await?;
            *fixture.policy.decision.lock().unwrap() = Ok(true);
            fixture
                .policy
                .revoke_after_first_check
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let (status, body) = fixture.upload(Some(group), "hello").await?;
            // PUT 전 인가는 성공했어도 등록 후 현재 권한이 없으면
            // 롤백·보상한다.
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(body["code"], "FORBIDDEN");
            let mut connection = fixture.database.db.conn().await?;
            assert_eq!(
                repository::list_by_group_id(&mut connection.handle(), group)
                    .await?
                    .len(),
                1
            );
            {
                let state = fixture.mock.state.lock().unwrap();
                assert!(state.objects.is_empty());
                assert_eq!(
                    state
                        .requests
                        .iter()
                        .filter(|(m, _)| m == Method::DELETE)
                        .count(),
                    1
                );
            }
            drop(connection);
            Ok(())
        })
        .await
    }
    #[tokio::test]
    async fn invalid_metadata_is_rejected_before_insertion() -> anyhow::Result<()> {
        use crate::app::files::feature::file_upload::usecase;
        crate::test_support::TestContext::run(4, move |database| async move {
            let fixture = Fixture::new(database, 8).await?;
            let result = usecase::execute(
                &fixture.container.db,
                fixture.container.file_storage.as_ref(),
                fixture.policy.as_ref(),
                &fixture.container.config.files,
                usecase::Command {
                    caller: UserClaims::new(fixture.database.user_id, UserStatus::Active, vec![]),
                    group_id: None,
                    files: vec![usecase::UploadFile {
                        original_filename: " ".into(),
                        content_type: "text/plain".into(),
                        body: Box::pin(std::io::Cursor::new(b"hello")),
                    }],
                },
            )
            .await;
            // HTTP 외 호출도 저장 전 모델 검증을 거치고 PUT 객체를 보상한다.
            assert_eq!(result.unwrap_err().code, Code::InvalidRequest);
            let mut connection = fixture.database.db.conn().await?;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conbira_files")
                .fetch_one(connection.handle().conn())
                .await?;
            assert_eq!(count, 0);
            assert!(fixture.mock.state.lock().unwrap().objects.is_empty());
            drop(connection);
            Ok(())
        })
        .await
    }
}
