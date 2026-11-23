# CodeGraph 설정

이 저장소에서는 로컬 CodeGraph CLI와 `codegraph` 스킬로 Rust 코드의 호출 관계와 변경 영향 범위를 추적한다. MCP 서버 설정은 필요 없다.

## CLI 설치

macOS 또는 Linux에서 [공식 설치 스크립트](https://github.com/colbymchenry/codegraph/blob/main/install.sh)를 실행한다.

```sh
curl -fsSL https://raw.githubusercontent.com/colbymchenry/codegraph/main/install.sh | sh
codegraph --version
```

`codegraph` 명령을 찾을 수 없으면 셸의 `PATH`에 `$HOME/.local/bin`을 추가한다.

## 저장소 인덱스 초기화

저장소 루트에서 실행한다. `init`은 `.codegraph/`에 초기 인덱스를 만들고, `status`는 인덱스 상태를 표시한다.

```sh
codegraph init .
codegraph status
```

Codex에서 `$codegraph`로 스킬을 호출할 수 있다. 검색어 선택과 명령 예제는 [스킬 문서](../../.agents/skills/codegraph/SKILL.md)를 참고한다.
