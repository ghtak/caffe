# 파일 공통 기반·HTTP API

Task 1의 설정·메타데이터·S3 기반 위에 Task 2 업로드·목록·다운로드·삭제와 Task 3 연결 확정 API 구현. 스펙은 현재 코드 관례와 사용자 수정 계약에 맞춰 해석.

## 책임

| 경계 | 구현 책임 |
| --- | --- |
| `config::Config` | 필수 인증·위치·정책 검증. endpoint 생략은 AWS 리전 기본값. HTTP는 localhost·loopback 모의 서버에만 허용 |
| `entity::File` | 메타데이터·귀속 쌍·attached 귀속·deleted 시각 검증. 허용 상태는 `FileStatus` |
| `repository` | 기존 `&mut db::Handle`로 조회·INSERT·논리 삭제 수행. 삭제용 단건 조회는 행 잠금 사용. 공통 `crate::Result` 사용. 자체 커밋·S3·인가 판단 없음 |
| `feature/file_upload.rs::usecase` | 서버 UUID 발급→PUT→현재 그룹 인가·귀속 상속→pending 생성·검증·저장→커밋·실패 보상 |
| `feature/file_group_attach.rs::usecase` | DB 기반 진입점의 Tx 시작·커밋, 연결·해제 검증과 일괄 변경. 내부 호출은 호출자 Tx에 참여 |
| `s3::S3ObjectAdapter` | AWS SDK로 조건부 PUT·HEAD·GET 서명·멱등 DELETE. 임시 파일로 스트림 크기를 제한하고 메모리 전체 적재 방지 |
| `access::ResourceAccessPort` | 콘텐츠 도메인의 read/edit 정책 위임. 실제 정책 연결 전 기본 deny, 전역 파일 우회 권한 없음 |
| `feature/file_*::endpoint` | 기존 UserClaims·공통 응답·UUID 추출기 사용. 업로드 multipart 전체 검증 후 디스크 spool 전달 |
| `feature/file_*::usecase` | 상태별 인가·DB 트랜잭션·S3 작업 순서·업로드 실패 보상 소유 |
| `Container` | DB·캐시 초기화 전에 파일 설정 검증·S3 및 리소스 인가 포트 조립. 생성 중 S3 호출 없음 |

위 책임과 앱 조립을 코드에 반영. (확인)

## 트랜잭션·스키마

- 별도 pending 등록 usecase 없이 업로드에서 메타데이터 생성·검증·저장과 트랜잭션·보상을 소유. usecase 인자는 전체 Container 대신 DB·S3·리소스 정책·필요 설정과 Command/Query로 구성. 업로드·목록은 공개 FileSummary 배열, 다운로드는 자체 Output 반환. (확인)
- 업로드 등록 흐름은 UUID 기반 파일 행 추가이며 그룹 귀속 변경 없음. 그룹·객체 위치 advisory lock과 중복 조회 제거. 기존 그룹은 존재 확인·귀속 상속을 위해서만 조회. (확인)
- `0013_files.sql`은 PK·NOT NULL·사용자 FK RESTRICT·조회 인덱스 유지. 사용자 지시에 따라 명시적 CHECK·객체 위치 UNIQUE 제거. 상태·메타데이터는 앱에서 검증하고, 객체 키는 호출자가 서버 발급 파일 UUID로 결정해 전달. 파일 ID 중복은 PK에서 거절. (확인)
- 그룹·파일 이력 조회는 deleted 행 포함. 모든 파일이 삭제된 그룹도 기존 그룹으로 조회하며 귀속·업로더 이력을 보존. 물리 삭제 함수 없음. (확인)
- 앱 규약을 거치지 않는 DB 직접 쓰기에는 제거한 CHECK·UNIQUE 조건이 보장되지 않음. (추론)

## S3·설정

- 인증은 `files.s3.access_key_id`, `secret_access_key` 필수. `CONBIRA_FILES__S3__ACCESS_KEY_ID`, `CONBIRA_FILES__S3__SECRET_ACCESS_KEY` 또는 `.env`에서 주입. Debug에서 두 값 가림. (확인)
- 파일 종류 제한 정책이 없어 `allowed_content_types` 설정 제거. 파일의 `content_type` 메타데이터와 S3 Content-Type 전달은 유지. (확인)
- PUT은 상한+1바이트까지만 임시 파일에 수신. 초과 시 S3 요청 없이 거절. `If-None-Match: *`로 기존 객체 덮어쓰기 차단. (확인)
- 서명은 supplied time·최대 7일 TTL을 사용한 로컬 GET 서명. 원본 파일명은 Content-Disposition에 percent encoding. HEAD·바이너리 다운로드를 내부 호출하지 않음. (확인)
- SDK 재시도는 비활성화. DELETE 호출마다 실제 요청 1회, 객체 없음도 성공. HEAD는 없음·권한/설정·일시 장애를 구분. (확인)
- 요청의 파일은 하나의 그룹으로 등록. group_id 생략 시 새 그룹 UUID 하나를 발급. PUT 중 실패하면 앞서 성공한 객체를 각각 1회 삭제. 등록 실패는 전체 롤백 확정 후 모든 성공 객체를 각각 1회 보상 삭제. 커밋 오류는 `500 INTERNAL`·객체 보존. 트랜잭션 ID·상태·파일 행으로 커밋 결과를 재확인하지 않음. 고아 객체 탐색·정리는 후속 작업. (확인)
- 운영 버킷·리전·접두사·인증·정책 값은 미확정. `config.toml`의 빈 값·0은 미설정 표시이며 앱 시작 시 거절. 테스트는 모의 설정 사용. (확인)

## HTTP API

| 경로 | 동작 |
| --- | --- |
| `POST /api/files` | file 1개 이상·max_files_per_upload 상한(현재 8개)·선택 group_id 1개, 순서 무관. 전체 multipart 검증·파일별 실측 바이트 제한 후 PUT·pending 일괄 등록·커밋, 입력 순 FileSummary 배열로 201 |
| `GET /api/file-groups/{group_id}/files` | 본인 pending·read 허용 attached만 생성 순 반환. 삭제 이력만 있는 그룹은 빈 배열, 미발급 그룹은 404 |
| `GET /api/files/{file_id}/download` | 상태별 인가 후 HEAD·GET 서명 URL 반환. deleted·없는 객체는 404 |
| `DELETE /api/files/{file_id}` | 파일 행 잠금 조회·상태별 인가·삭제 마킹·커밋 후 S3 삭제 1회. 인가된 중복 삭제는 변경 없이 성공 |
| `POST /api/file-groups/{group_id}/attach` | 본인 pending 연결·명시한 첨부 해제를 하나의 Tx로 확정. S3 호출 없음 |

- 요청당 파일 개수는 `files.max_files_per_upload = 8`로 제한. 0은 설정 오류. multipart 수신 중 초과 파일의 임시 저장 전에 `400 INVALID_REQUEST` 반환하며 전체 요청을 거절하고 S3·DB 등록 없음. 환경 변수 `CONBIRA_FILES__MAX_FILES_PER_UPLOAD`로 변경 가능.
- 5개 경로에 기존 Bearer 인증과 `Cache-Control: no-store` 적용. UUID·multipart·JSON 오류도 공통 JSON 봉투 사용. 공개 `FileSummary`에는 저장 위치·업로더·리소스 귀속 제외. (확인)
- pending은 업로더만 접근. attached는 콘텐츠 read/edit 필요. 삭제 이력의 중복 삭제는 업로더 또는 귀속 리소스 편집자에게 허용. (확인)
- 목록은 S3를 호출하지 않음. 리소스 인가 장애는 전체 503, 거부와 구분. 실제 콘텐츠 정책은 아직 연결하지 않아 귀속 리소스 접근은 기본 거부. (확인)
- 논리 삭제 커밋 후 물리 삭제 실패는 경고만 남기고 HTTP 성공 유지. 커밋 실패는 S3 삭제 없이 500. 행 이력과 객체 위치는 보존. (확인)
- 삭제 객체 재시도·pending 정리는 Task 4 범위. 메타데이터 없는 고아 객체 정리는 별도 후속 추가 대상. (확인)

## 연결 확정·내부 호출

- HTTP Endpoint는 인증·입력 추출·응답을 담당. `files::file_group_attach::execute(&Database, policy, ttl, Command)`가 Tx 시작·커밋을 담당. 콘텐츠 저장과 결합할 때는 `execute_in_tx(&mut Handle, policy, ttl, Command)`를 활성 호출자 Tx에서 사용. 오류 발생 시 호출자가 전체 Tx 롤백을 담당. Handle은 기존 타입을 재사용하므로 활성 Tx 여부를 타입으로 보장하지 않음. (확인)
- 요청은 `resource_type`, `resource_key`, `attach_file_ids`, 생략 시 빈 배열인 `detach_file_ids`로 구성. 빈 귀속·배열 중복·배열 간 교집합 거절. 응답은 `group_id`, 입력 순서의 `attached_file_ids`, `detached_file_ids`를 포함. (확인)
- 본인 pending만 연결하며 TTL 도달 파일은 거절. attached·deleted 해제는 업로더 본인 또는 원본 리소스 Edit 권한자에게 허용. deleted 재요청은 시각을 유지하고 응답 ID에 포함. 기존 DELETE 인가 계약은 유지. (확인)
- 선택하지 않은 파일은 변경하지 않음. 그룹 조회는 삭제 이력을 포함한 non-NULL 귀속을 우선하며 저장된 귀속이 여러 개면 INTERNAL 반환. 빈 요청은 그룹·귀속만 확인하며 미귀속 그룹을 귀속시키지 않음. (확인)
- 사용자 지시에 따라 문서의 DB 경합 요구·동시성 시나리오 검증 제외. 기존 Read Committed 유지, 신규 애플리케이션 락·행 잠금·버전 관리·DDL 없음. 상태 조건부 UPDATE와 변경 건수 검사만 수행. 서로 다른 파일의 동시 최초 연결을 직렬화하지 않음. (확인)

## 검증

- 기존 Task 1 테스트와 실제 앱 라우터·PostgreSQL·모의 S3·모의 인가 정책으로 HTTP 경계 검증. 서명 URL은 실제 HTTP GET으로 확인. 모의 S3는 AWS 서명 검증기를 재현하지 않음.
- 테스트별 독립 스키마 사용. 실패 주입용 제약·트리거는 테스트 스키마에만 생성하며 운영 DDL 변경 없음.
- 실행 명령: `cargo check -p conbira --all-targets`, `cargo clippy -p conbira --all-targets -- -D warnings`, `cargo test -p conbira`.
- 기존 테스트 서버에 터미널(`hc`) 경로로 연결해 `cargo test -p conbira` 통과: 라이브러리 143개·CLI 4개, 실패 없음. 포맷·check·clippy `-D warnings` 통과. (확인)
- Task 3 추가 후 같은 터미널 경로로 `cargo test -p conbira` 통과: 신규 연결 테스트 21개를 포함해 라이브러리 164개·CLI 4개. 연결·해제·권한·만료·그룹 귀속·커밋 실패·변경 건수 부족·호출자 Tx 커밋/롤백 확인. (확인)
