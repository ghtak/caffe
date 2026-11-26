use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse as _, Response};
use uuid::Uuid;

use crate::app::files::config::{Config, S3Config};

pub fn config(endpoint: Option<String>) -> Config {
    Config {
        s3: S3Config {
            access_key_id: "test-access-key-id".into(),
            secret_access_key: "test-secret-access-key".into(),
            bucket: "conbira-test".into(),
            region: "us-east-1".into(),
            endpoint,
            key_prefix: "test-files".into(),
        },
        pending_ttl_seconds: 3600,
        cleanup_interval_seconds: 60,
        download_url_ttl_seconds: 300,
        max_file_size_bytes: 1024 * 1024,
        max_files_per_upload: 8,
    }
}

pub struct FileFixture {
    pub db: Arc<crate::app::db::Database>,
    pub user_id: i64,
}

impl FileFixture {
    pub async fn new(database: Arc<super::TestContext>) -> anyhow::Result<Self> {
        let mut connection = database.db.conn().await?;
        let user_id = sqlx::query_scalar(
            "INSERT INTO conbira_auth_users (nickname) VALUES ('file owner') RETURNING id",
        )
        .fetch_one(connection.handle().conn())
        .await?;
        Ok(Self {
            db: Arc::clone(&database.db),
            user_id,
        })
    }

    pub fn file(&self, group_id: Uuid) -> crate::app::files::entity::File {
        let file_id = Uuid::new_v4();
        let now = jiff::Timestamp::now();
        crate::app::files::entity::File {
            file_id,
            group_id,
            resource_type: None,
            resource_key: None,
            status: crate::app::files::entity::FileStatus::Pending,
            uploaded_by: self.user_id,
            s3_bucket: "conbira-test".into(),
            s3_object_key: format!("test-files/{file_id}"),
            original_filename: "report.txt".into(),
            size_bytes: 5,
            content_type: "text/plain".into(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[derive(Default)]
pub struct MockState {
    pub objects: HashMap<String, (Bytes, String)>,
    pub requests: Vec<(Method, String)>,
    pub failures: HashMap<(Method, String), StatusCode>,
    pub method_failures: HashMap<Method, StatusCode>,
    pub fail_put_number: Option<usize>,
}

pub struct MockS3 {
    pub endpoint: String,
    pub state: Arc<Mutex<MockState>>,
    task: tokio::task::JoinHandle<()>,
}

impl MockS3 {
    pub async fn new() -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let endpoint = format!("http://{}", listener.local_addr()?);
        let state = Arc::new(Mutex::new(MockState::default()));
        let app = axum::Router::new()
            .fallback(mock_object)
            .with_state(state.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Ok(Self {
            endpoint,
            state,
            task,
        })
    }

    pub async fn adapter(&self) -> crate::Result<crate::app::files::s3::S3ObjectAdapter> {
        crate::app::files::s3::S3ObjectAdapter::new(&config(Some(self.endpoint.clone()))).await
    }

    pub fn location(&self, key: &str) -> crate::app::files::s3::ObjectLocation {
        crate::app::files::s3::ObjectLocation {
            bucket: "conbira-test".into(),
            key: key.into(),
        }
    }
}

impl Drop for MockS3 {
    fn drop(&mut self) { self.task.abort(); }
}

async fn mock_object(State(state): State<Arc<Mutex<MockState>>>, request: Request) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let headers = request.headers().clone();
    let query = request.uri().query().unwrap_or_default().to_owned();
    let body = axum::body::to_bytes(request.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    let mut state = state.lock().unwrap();
    state.requests.push((method.clone(), path.clone()));
    if let Some(status) = state
        .failures
        .get(&(method.clone(), path.clone()))
        .or_else(|| state.method_failures.get(&method))
    {
        return (*status, "<Error><Code>InjectedFailure</Code></Error>").into_response();
    }
    if method == Method::PUT
        && state.fail_put_number
            == Some(
                state
                    .requests
                    .iter()
                    .filter(|(m, _)| m == Method::PUT)
                    .count(),
            )
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "<Error><Code>InjectedFailure</Code></Error>",
        )
            .into_response();
    }
    match method {
        Method::PUT => {
            if headers.get("if-none-match").and_then(|h| h.to_str().ok()) != Some("*") {
                return (
                    StatusCode::BAD_REQUEST,
                    "<Error><Code>InvalidRequest</Code></Error>",
                )
                    .into_response();
            }
            if state.objects.contains_key(&path) {
                return (
                    StatusCode::PRECONDITION_FAILED,
                    "<Error><Code>PreconditionFailed</Code></Error>",
                )
                    .into_response();
            }
            let content_type = headers
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            state.objects.insert(path, (body, content_type));
            (StatusCode::OK, [("etag", "\"mock-etag\"")]).into_response()
        }
        Method::HEAD | Method::GET => {
            if method == Method::GET && !query.contains("X-Amz-Signature=") {
                return StatusCode::FORBIDDEN.into_response();
            }
            match state.objects.get(&path) {
                Some((bytes, mime)) => (
                    [
                        ("content-type", mime.clone()),
                        ("content-length", bytes.len().to_string()),
                    ],
                    if method == Method::HEAD {
                        Bytes::new()
                    } else {
                        bytes.clone()
                    },
                )
                    .into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
        Method::DELETE => {
            state.objects.remove(&path);
            StatusCode::NO_CONTENT.into_response()
        }
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}
