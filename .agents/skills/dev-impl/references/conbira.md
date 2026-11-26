# Conbira Rust 모듈 구조 및 구현 관례

신규 기능·도메인 모듈 추가 시 임의의 수평 레이어(`controllers/`, `models/`, `dto/`, `services/` 등)를 만들지 않고 아래 구조를 따른다.

## 1. 크레이트 및 앱 레이아웃

```text
conbira/src/
├── lib.rs              # here! 매크로, pub mod {app, config, error, option}, pub use Error/Code/Result
├── main.rs / cli/      # 바이너리 진입점 및 CLI(migration 등)
├── config.rs           # 최상위 Config (도메인별 Config 조합)
├── error.rs            # Code, Error, Result, ResultExt (map_err_internal, map_err_invalid_request)
├── test_support.rs     # #[cfg(test)] 공통 테스트 컨테이너·DB 설정
└── app/
    ├── mod.rs          # 도메인 모듈 선언, pub use container::*
    ├── container.rs    # Container (의존성 조립), db::{Database, Handle, QueryBuilder}, DatabaseExt
    ├── http.rs         # router(Arc<Container>)에서 /api/<domain> nest 마운트
    ├── http/
    │   ├── extract.rs  # ApiJson, ApiQuery, ApiPath
    │   └── response.rs # ApiResult<T>, ApiResponse<T>
    └── <domain>/       # 도메인 단위 수직 슬라이스 (auth, files, rbac, sample 등)
```

## 2. 도메인 모듈 표준 구조 (`src/app/<domain>/`)

도메인 규모에 따라 단일 파일 또는 하위 디렉토리로 구성한다.

```text
src/app/<domain>/
├── mod.rs              # 하위 모듈 선언, pub fn router() -> axum::Router<Arc<Container>>
├── entity.rs           # 엔터티·상태 enum·도메인 불변식 검증 (sqlx::FromRow, serde::Serialize)
├── repository.rs       # 단일 파일 또는 repository/*.rs (DB 접근 전담)
├── feature/            # 비공개(mod feature;). 유스케이스별 1파일 (<action>.rs)
│   ├── mod.rs
│   └── <action>.rs     # endpoint 모듈 (+ 필요 시 usecase 모듈)
├── service.rs          # 선택. 복수 feature 공유 도메인/인프라 서비스 (또는 service/*.rs)
├── config.rs           # 선택. 도메인 설정 구조체
├── <port>.rs           # 선택. 외부 시스템·도메인 간 경계 포트/어댑터 (예: s3.rs, access.rs)
└── tests/              # 선택. 도메인 통합/HTTP 테스트 (또는 각 파일 내 #[cfg(test)])
```

### `mod.rs` 규칙
- `feature`는 반드시 비공개 `mod feature;`로 선언한다.
- 외부 도메인·CLI에서 재사용하는 유스케이스만 선택적으로 재노출한다.
  - 예: `pub use feature::file_group_attach::usecase as file_group_attach;`
- `pub fn router() -> axum::Router<Arc<Container>>`에서 `feature::<action>::endpoint::handler`를 연결한다.
- 상위 라우터 등록은 `src/app/http.rs`의 `router()`에 `.nest("/api/<domain>", super::<domain>::router())`로 추가한다.

## 3. 계층별 책임 및 시그니처 관례

### `feature/<action>.rs` (`endpoint` / `usecase`)
- **파일 명명**: `<자원>_<동작>.rs` (예: `sample_create.rs`, `file_upload.rs`, `role_permission_add.rs`).
- **`pub mod endpoint`**:
  - `RequestBody` / `RequestQuery` (`Deserialize`, `espr_derive::Validate`), `ResponseData` 타입 정의.
  - `pub async fn handler(State(container): State<Arc<Container>>, ...) -> ApiResult<ResponseData>`
  - 인증은 `UserClaims` 추출기, 입력은 `ApiJson`/`ApiQuery`/`ApiPath`, 반환은 `ApiResponse::from(...).into()` 또는 `Ok(result.into())` 사용.
  - 단순 CRUD(예: `rbac/feature/role_create.rs`)는 `usecase` 없이 `endpoint::handler`에서 권한 확인 후 `repository`를 직접 호출할 수 있다.
- **`pub mod usecase`** (트랜잭션 경계·복수 저장소·외부 포트 조율이 필요한 경우):
  - `Command` 또는 `Query` 입력 구조체 정의.
  - `pub async fn execute(...) -> crate::Result<...>`
  - **의존성 주입 원칙**: `Arc<Container>` 전체를 받지 않고 필요한 의존성만 명시적 인자로 받는다.
    - 예: `db: &Database`, `storage: &dyn S3ObjectPort`, `config: &Config`, `command: Command`
  - **트랜잭션 관리**:
    - 독립 진입점은 `let mut tx = db.begin_tx(crate::here!()).await?;` → `tx.handle()` → `tx.commit().await`.
    - 다른 유스케이스의 트랜잭션에 참여해야 하면 `execute_in_tx(handle: &mut Handle<'_>, ...)`를 함께 제공한다.

### `repository` (`repository.rs` 또는 `repository/*.rs`)
- 모든 함수는 첫 인자로 `handle: &mut crate::app::container::db::Handle<'_>`를 받는다.
- 반환 타입은 `crate::Result<T>`를 사용하며, SQLx 오류는 `.map_err_internal("...", crate::here!())`로 변환한다.
- 자체 트랜잭션 시작·커밋, 인가 판단, 외부 API 호출을 하지 않는다.

### `entity.rs`
- DB 행 매핑(`#[derive(sqlx::FromRow)]`) 및 응답 직렬화(`serde::Serialize`) 타입을 둔다.
- 타임스탬프는 `#[sqlx(try_from = "jiff_sqlx::Timestamp")] pub created_at: jiff::Timestamp` 관례를 따른다.
- 상태 전이·불변식 검증은 엔터티 메서드에서 수행하고 위반 시 `crate::Error::new(Code::InvalidRequest, ...).at(crate::here!())`를 반환한다.

### 외부/도메인 경계 포트 (`<port>.rs`) 및 `Container`
- S3, 외부 API, 타 도메인 정책 등 교체·모의가 필요한 경계는 `<trait>Port`(예: `S3ObjectPort`, `ResourceAccessPort`)와 `<impl>Adapter`로 정의한다.
- 조립은 `src/app/container.rs`의 `Container::new`에서 수행하고, `Container` 필드로 `Arc<dyn ...Port>` 또는 서비스 인스턴스를 보유한다.

### 오류 및 테스트 관례
- 오류 생성: `Error::new(Code::..., "메시지").at(crate::here!())` 또는 `ResultExt` (`map_err_internal`, `map_err_invalid_request`).
- 테스트: `#[cfg(test)] mod tests { mod contract { ... } }` 구조 사용.
  - DB 테스트는 `CREATE TABLE IF NOT EXISTS`를 `CREATE TEMPORARY TABLE`로 치환해 실행한다.
  - HTTP 경계 테스트는 `app::http::router(container)`에 `tower::ServiceExt::oneshot`으로 검증한다.
