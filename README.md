# ☕ caffe

**Rust로 서버 애플리케이션을 만들기 위한 도구와 예제 워크스페이스**

설정, HTTP 서버, 데이터베이스 연결, 인증과 로깅처럼 서버에서 자주 필요한 기능을 모으고, 이를 실제 애플리케이션에서 사용하는 방법을 함께 살펴봅니다.

---

## 구성

| 프로젝트 | 설명 |
| --- | --- |
| **espr** | 서버 애플리케이션을 위한 공통 기능 라이브러리 |
| **espr-derive** | `espr`과 함께 사용하는 derive 매크로 |
| **espr-cli** | HTTP·HTTPS 서버와 공통 기능 사용 예제 |
| **conbira** | `espr`을 활용한 샘플 서버와 관리 CLI |

## 개발

[Rust](https://www.rust-lang.org/)와 [just](https://github.com/casey/just)를 설치한 뒤 워크스페이스 루트에서 실행합니다.

```sh
just           # 사용 가능한 명령 보기
just check     # 포맷 및 Clippy 검사
just test      # 기본 테스트 실행
```

`just check`에는 nightly Rust가 필요합니다. `just test`에는 [테스트용 PostgreSQL과 Valkey](tests/docker/docker-compose.yml)가 필요합니다. 기본 호스트는 `192.168.1.20`이며 PostgreSQL은 `5432`, Valkey는 `6379`를 사용합니다.

espr는 `espr_test`, conbira는 `conbira_test` DB를 사용합니다. Compose는 새 볼륨에서 두 DB를 생성합니다. 기존 볼륨을 사용하면 두 DB를 한 번 직접 생성해야 합니다.

다른 PostgreSQL을 사용할 때는 `CAFFE_DATABASE_URL`에 테스트 DB의 전체 주소를 지정합니다. 워크스페이스 전체를 실행하면 두 크레이트에 같은 주소가 적용되므로, DB를 분리하려면 주소를 바꿔 각 크레이트의 `cargo test -p <crate>`를 실행합니다. Valkey 주소는 `CAFFE_CACHE_URL`로 재정의할 수 있습니다.

마이그레이션을 검증할 때는 별도로 준비한 빈 DB의 주소를 `CONBIRA_DATABASE__URL`로 지정하고 `just conbira-cli-migration`을 실행합니다.

Oracle 테스트 환경이 필요하면 Compose의 `oracle` 서비스 블록 주석을 해제하고 `docker compose -f tests/docker/docker-compose.yml up -d --wait oracle`로 시작합니다. 기본 접속 주소는 `192.168.1.20:31521/FREEPDB1`이며, Compose 예시의 `APP_USER`와 `APP_USER_PASSWORD`를 사용합니다. 접속 설정은 `CAFFE_ORACLE_CONNECT_STRING`, `CAFFE_ORACLE_USERNAME`, `CAFFE_ORACLE_PASSWORD`로 바꿀 수 있습니다. Oracle 테스트만 실행하려면 `just test-ignored`, 전체 테스트를 실행하려면 `just test-all`을 사용합니다.
