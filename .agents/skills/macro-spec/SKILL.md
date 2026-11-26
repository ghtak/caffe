---
name: macro-spec
description: Generate a shared-foundation full-stack (BE and FE) macro architecture and system specification document (공통 조건, BE/FE 확장 슬롯, HTTP 경계 계약, 전체 Mermaid, 간결한 태스크 인덱스) from a requirement document (req-seed or req-def). Use when the user asks for 매크로 설계, 공통 설계, 시스템 스펙 정의, 태스크 분리, or wants a stable architectural baseline with extension points and lightweight parallel tasks for rust-impl and fe-impl.
---

# macro-spec

Transform a requirement document (`req-seed` or `req-def`) into a stable Shared-Foundation Macro Specification. This document fixes shared infrastructure constraints, BE/FE extension slots, HTTP boundary contracts, and system-level Mermaid flows so individual tasks (`rust-impl` / `fe-impl`) can run in parallel and future features can plug in without rewriting the spec.

## When to Use

- The user provides a `req-seed` / `req-def` document or high-level feature scope and asks for a macro design (`매크로 설계`, `공통 설계`) or system specification (`시스템 스펙`).
- The user wants a durable architectural baseline with clear extension points and a lightweight task index (target, prerequisites, completion criteria) for parallel agent execution.

## Standardized Output Format

Always output the document using this exact top-level Markdown structure (if the scope is strictly BE-only or FE-only, omit the unused side's sub-items cleanly):

````markdown
# 매크로 설계: [기능/도메인명]

## 1. 공통 조건 및 불변 제약

### 환경 및 외부 연동 설정 (Config / Env)
- BE 설정: [외부 연동 `.env` 키 계층 및 `Config` 구조 요약]
- FE 설정: [클라이언트 공개 환경 변수(`VITE_*`) 또는 동일 오리진 `/api` 프록시 기준]
- [미확정 외부 입력값은 `{사용자가 입력}` 형태로 명시]

### 데이터 영속성 및 상태 제약 (Persistence & State)
- RDB 불변 제약: [공통 테이블, 핵심 식별 컬럼, PK/FK/Unique 제약 요약 - 기본 격리 수준(Read Committed)·PK/FK/Unique·조건부 UPDATE만 사용하며, 요구사항에 없는 잠금(`FOR UPDATE`, advisory lock)·`version` 컬럼·전체 DDL 생략]
- In-Memory / Cache (Redis 등, 해당 시): [키 네임스페이스 패턴, 저장 목적, TTL 정책 요약]
- FE 라우트 및 서버 상태: [공통 라우트 가드 기준, 인증 상태 및 무효화할 서버 상태(`queryKey`) 규칙]

---

## 2. 확장 슬롯 및 경계 계약 (Extension & Boundary Contracts)

### BE / FE 확장 슬롯 (Extension Points)
- BE 확장 지점: [신규 공급자·슬라이스가 연결되는 모듈 경로, Enum/라우터 마운트, 공유 세션/인프라 경계]
- FE 확장 지점: [신규 화면·기능이 연결되는 `features/<domain>/` 배럴(`index.ts`), 라우트 트리, 레이아웃/프로바이더 슬롯]

### BE - FE HTTP API 경계 계약
| 메서드·경로 | 요청·인증 | 성공 응답 및 상태 분기 | 오류 코드 분기 |
| --- | --- | --- | --- |
| `[METHOD /path]` | [헤더/본문 요약] | [상태 코드 및 응답 DTO/분기 필드] | [주요 비즈니스·인증 오류 코드] |

---

## 3. 전체 인터페이스 연계 흐름

```mermaid
sequenceDiagram
    %% FE 화면, BE HTTP API, 외부 시스템, 저장소(DB/Redis) 간의 경계 호출 흐름만 표현

```

---

## 4. 태스크 인덱스 (Task Map)

### Task 1 [BE]: [태스크명 - 예: 공통 설정, 영속성 제약 및 외부 어댑터]

* 대상: [핵심 디렉터리/모듈 요약]
* 선행 조건: 없음 (또는 외부 의존 조건)
* 완료 조건: [빌드/정적 검사 명령 + 모의 외부 서버/DB/Redis 블랙박스 검증 기준]

### Task 2 [BE]: [태스크명 - 예: 기능 슬라이스 HTTP API]

* 대상: [관련 슬라이스 및 라우터 경로]
* 선행 조건: Task 1
* 완료 조건: [테스트 명령 + 정상/예외 시나리오 API 계약 검증 기준]

### Task 3 [FE]: [태스크명 - 예: 화면 라우트, API 연동 및 시나리오 테스트]

* 대상: [관련 `features/<domain>/` 및 앱 조립 경로]
* 선행 조건: BE - FE HTTP API 경계 계약 확정 (BE 구현과 병렬 실행 가능)
* 완료 조건: [타입/테스트 명령 + MSW 기반 정상/예외 플로우 및 라우트 가드 검증 기준]

```

## Workflow Steps

1. Generate Stable Baseline Immediately: Produce the macro specification directly from the provided requirements without preliminary Q&A.
2. Respect Existing Stack & Conventions: Apply known project conventions (e.g., Vertical Slice Architecture, feature barrel exports, existing table/env prefixes) directly when available.
3. Design for Extension, Not White-Box Detail: Define where future providers or slices plug in (Extension Points) and fix external BE-FE HTTP DTOs and DB/Redis invariants. Omit internal function signatures, helper structs, transaction lock ordering, and client storage internals (`sessionStorage` details)—leave those to `rust-impl` and `fe-impl`.
4. Avoid Speculative Concurrency Defenses: Unless the requirement explicitly specifies a high-contention scenario (e.g., inventory deduction, seat reservation, balance withdrawal), rely solely on DB default isolation (`Read Committed`), PK/FK/Unique constraints, and conditional `UPDATE` (`WHERE status = ...`). Do not invent `SELECT FOR UPDATE`, optimistic lock `version` columns, advisory/distributed locks, idempotency keys, or retry/compensation queues. If concurrency requirements are genuinely ambiguous, surface them as `{UserInputRequired}` instead of assuming locks.
5. Mark External/User-Supplied Values Explicitly: Never fabricate specific URLs, callback paths, secret keys, or policy texts that were not provided. Use explicit placeholders like `{사용자가 입력}` or `{사용자가 확정}`.
6. Keep Mermaid at System Boundary Level: Show interactions across FE pages, BE HTTP endpoints, external APIs, and persistence stores. Omit internal service/repository helper calls.
7. Keep Task Index Lightweight (3-Line Summary): Prefix tasks with `[BE]` or `[FE]` and list only `대상`, `선행 조건`, and `완료 조건` so tasks remain decoupled from internal implementation churn and can be dispatched directly to `rust-impl` or `fe-impl`.
8. Surface Missing Spec Inputs (If Needed): List 1 to 3 concise `{UserInputRequired}` items at the very bottom after `---` only when missing external inputs or domain policies block implementation.

## Gotchas

- Bloating the task list into mini-specs: Keep each task strictly to the 3-line summary (`대상`, `선행 조건`, `완료 조건`). Detailed contracts live in Sections 1 and 2; code skeletons live in `rust-impl` / `fe-impl`.
- Leaking white-box design into macro-spec: If an internal detail can change during refactoring without breaking BE-FE HTTP contracts, extension slots, or DB schemas, remove it from `macro-spec`.
- Writing full implementation code: Never include function bodies, React JSX markup, or SQL DDL scripts.
- Inventing unrequested DB locks or race-condition defenses: Never add row locks, `version` columns, distributed locks, or retry loops for hypothetical contention not stated in the requirements.
- No em-dashes or en-dashes: Use standard hyphens (`-`) or middle dots (`·`) for lists and separators.
