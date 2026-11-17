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

`just check`에는 nightly Rust가 필요합니다. 외부 서비스가 필요한 테스트를 포함하려면 `just test-all`을 사용합니다.
