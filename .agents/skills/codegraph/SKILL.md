---
name: codegraph
description: Explore Rust definitions, callers, callees, and change impact in Caffe using the local CodeGraph CLI. Use when tracing feature flows, finding usages of domain types or repositories, or checking dependencies before refactoring.
---

# CodeGraph 탐색

저장소 루트에서 실행한다. Rust 모듈 경로로 대상을 식별하고, CodeGraph에는 정의 파일과 인덱스에 기록된 심볼명을 전달한다.

## 검색어 선택

- 구체적인 타입이나 함수 이름을 알면 `codegraph query TokenService --kind struct --limit 5`처럼 정의 후보를 찾는다.
- 기능명만 알면 `rg --files <크레이트>/src` 결과를 기능명으로 좁힌다. `conbira`의 인라인 `endpoint`와 `usecase`는 해당 기능 파일에서 확인한다.
- `espr::jwt::Config`처럼 Rust 경로를 알면 `mod`, `use`, 별칭, `pub use`를 따라 실제 정의 파일을 찾는다. `Config`, `Error`, `new`, `handler`, `execute`처럼 반복되는 이름은 `--file`과 함께 조회한다. 인라인 모듈의 `usecase::execute`는 정의 파일과 `execute`를 전달한다.

```sh
codegraph node Config --file espr/src/jwt.rs
codegraph node execute --file conbira/src/app/sample/feature/sample_create.rs
```

## 관계 추적

확인한 정의를 기준으로 호출자, 피호출자, 영향 범위를 조회한다. 동명 정의로 범위가 넓어졌거나 예상한 연결이 빠졌으면 결과의 파일 경로, 호출부와 `use` 선언을 확인한다.

```sh
codegraph callers execute --file conbira/src/app/sample/feature/sample_create.rs --limit 5
codegraph callees execute --file conbira/src/app/sample/feature/sample_create.rs --limit 5
codegraph impact execute --file conbira/src/app/sample/feature/sample_create.rs --depth 1
```

- 파일의 심볼 목록과 의존 파일은 `codegraph node espr/src/jwt.rs --symbols-only`로 확인한다. 관련 코드의 후보를 모을 때는 `codegraph context TokenService --max-nodes 8`처럼 확인한 이름으로 시작한다. 결과는 탐색 단서로 활용하고, 대상 정의와 연결은 파일을 지정해 확인한다.
- 인덱스가 오래됐으면 `codegraph status`로 상태를 확인하고 `codegraph sync .`으로 변경분을 반영한다. 전체 재구축이 필요한 경우 `codegraph index .`를 사용한다.
