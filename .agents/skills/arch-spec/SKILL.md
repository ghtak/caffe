---
name: arch-spec
description: 요구사항이나 기능 아이디어를 바탕으로 DBML+Mermaid 시각적 경계 초안을 스케치하고, 데이터 소유권·협업 인터페이스·예외 정책·완료조건·검증 매트릭스가 통합된 TypeScript 기반 메타 스펙(spec.ts)을 생성합니다.
---

# Architecture Spec Generator (`arch-spec`)

자연어 요구사항을 모호함 없는 결정론적 아키텍처 계약(`spec.ts`)으로 변환한다.
이 스킬은 설계 경계 확정까지만 책임지며, 실제 타깃 언어 구현 코드는 작성하지 않는다.

## 핵심 원칙

1. 일회용 설계 계약서: 생성된 스펙은 구현 에이전트에게 넘길 명확한 울타리 역할을 수행하며, 구현 완료 후 시스템의 SSOT(단일 진실 공급원)는 실제 코드와 DDL로 이관된다.
2. 추측성 타입·동시성 방어 배제: 불확실한 `Option`/nullable이나 임의의 방어적 분기를 추가하지 않는다. 요구사항에 명시된 고경합 시나리오(재고 차감, 선착순 등)가 없으면 DB 기본 격리 수준(`Read Committed`)·PK/FK/Unique 제약·조건부 `UPDATE`만 사용하며, 추측성 `SELECT FOR UPDATE`·낙관적 락(`version` 컬럼)·분산/advisory lock·임의의 `CONFLICT`/`BUSY` 에러 코드를 설계하지 않는다. 불명확한 경계나 동시성 요구는 초안 단계에서 사용자에게 확인한다.
3. 단일 파일 완결성: 최종 스펙(`spec.ts`) 하나에 데이터 소유권, 입출력 계약, 예외 정책, 완료 조건, 필수 검증 케이스를 모두 선언한다.

---

## 진행 단계

### Stage 1. 경계 스케치 (`DBML + Mermaid`)
입력된 요구사항을 분석해 데이터 소유권과 컴포넌트 간 협업 방향의 초안을 제시한다.

1. DBML (데이터 모델 및 소유권):
   * 신규 또는 변경되는 테이블, 핵심 컬럼, FK 관계를 정의한다.
   * 모든 테이블 `note`에 소유 스토어(`owner: <store_name>`)를 명시해 도메인 경계를 고정한다.
2. Mermaid (협업 및 메시지 흐름):
   * `flowchart LR` 형식으로 `Endpoint -> UseCase -> Store / External Port` 간의 호출 방향과 데이터 접근 경계를 그린다.
3. 사용자 확인 게이트:
   * 테이블 분리 기준, 스토어 소유권, 외부 연동 경계에 모호함이 있으면 초안 제시 후 사용자의 경계 확정을 기다린다.

---

### Stage 2. 최종 메타 스펙 확정 (`spec.ts`)
확정된 경계를 바탕으로 아래 5가지 섹션이 모두 포함된 순수 TypeScript 타입 스펙을 작성한다.

1. DB 스키마 및 소유권 (`namespace DB`):
   * `@owner <store_name>` JSDoc 태그로 테이블별 접근 권한을 못 박는다.
2. 협력 컴포넌트 경계 (`Store` / `Port` Interfaces):
   * 슬라이스가 의존하는 내부 저장소와 외부 연동 포트의 공개 시그니처만 선언한다.
3. 슬라이스 입출력 계약 (`Req` / `Res` / `Err`):
   * 입력(`Req`), 분기별 응답 유니온(`Res`), 발생 가능한 도메인 실패 리터럴 유니온(`Err`)을 정의한다.
4. 예외 처리 정책 (`ERROR_POLICY`) 및 완료 조건 (`@tx`, `@ensures`):
   * `ERROR_POLICY`: 각 `Err` 코드별 HTTP 상태 코드와 트랜잭션 롤백 여부를 상수 맵으로 선언한다.
   * `@tx`: 트랜잭션 원자성 범위(`atomic` 또는 `none`)를 명시한다. `atomic`은 단일 요청의 DB 기본 격리 수준(`Read Committed`) 원자성만 의미하며, 별도 명시가 없는 한 추가 행 잠금이나 직렬화를 전제하지 않는다.
   * `@ensures`: 구현 방식(How)이 아닌 단일 요청 실행 종료 시점에 DB와 응답이 만족해야 할 결과 불변식(What)을 기술한다.
5. 검증 매트릭스 (`VERIFICATION_CASES`):
   * 구현 단계에서 반드시 작성·통과해야 할 `contract`(정상 계약) 및 `edge`(경계·실패) 테스트 목록을 입력(`given`)-기대결과(`expect`) 구조로 고정한다. 요구사항에 없는 동시성 경합·레이스 컨디션 `edge` 케이스를 임의로 추가하지 않는다.

---

## 출력 템플릿 (`spec.ts`)

```typescript
// 1. DB 스키마 및 소유권 (Storage Boundary)
namespace DB {
  / @owner <store_name> */
  export interface <EntityName> {
    id: <IdType>;
    status: "<STATE_A>" | "<STATE_B>";
  }
}

// 2. 협력 컴포넌트 경계 (Collaborators)
interface <CollaboratorPort> {
  <action>(input: <Input>): Promise<<Output>>;
}

// 3. 기능 슬라이스 통합 계약 (Vertical Slice Contract)
export namespace <FeatureName> {
  export type Req = {
    // 필수 입력 필드만 명시
  };

  export type Res =
    | { kind: "<CASE_A>"; /* ... */ }
    | { kind: "<CASE_B>"; /* ... */ };

  // 4-1. 예외 처리 계약
  export type Err = "<ERR_CODE_1>" | "<ERR_CODE_2>";

  export const ERROR_POLICY: Record<Err, { http: number; rollback: boolean }> = {
    <ERR_CODE_1>: { http: 400, rollback: true },
    <ERR_CODE_2>: { http: 403, rollback: true },
  };

  // 4-2. 완료 조건 (사후 불변식)
  /
   * @tx atomic (<대상 테이블 목록>)
   * @collaborators <CollaboratorPort 목록>
   * @ensures <실행 완료 시 반드시 성립해야 할 상태 조건 1>
   * @ensures <실행 완료 시 반드시 성립해야 할 상태 조건 2>
   */
  export interface UseCase {
    execute(req: Req): Promise<Res Err |>;
  }

  // 5. 검증 조건 (필수 테스트 매트릭스)
  export const VERIFICATION_CASES = [
    { kind: "contract", id: "<case_id_1>", given: "<입력 조건>", expect: "<기대 상태 및 응답>" },
    { kind: "edge",     id: "<case_id_2>", given: "<실패/경계 조건>", expect: "<에러 코드 및 롤백 결과>" },
  ] as const;
}

```
