---
name: req-seed
description: Create a structured, implementation-ready feature specification document (개요 및 목적, 핵심 요구사항, 상세 정책 및 스펙, 사용자 시나리오, 예외 처리 시나리오) from rough feature ideas or notes. Use when the user asks to organize initial requirements into a structured spec sheet, define business rules and edge cases, or prepare a baseline document for design and development.
---

# req-seed

Transform rough feature ideas, notes, or fragmented prompts into a standardized, structured feature specification document. This document bridges the gap between raw ideas and implementation by systematically defining objectives, user stories, core business rules, step-by-step user flows, and edge cases.

## When to Use

- The user shares rough notes or an idea and wants to organize them into a structured specification document.
- The user needs to clarify actors, business policies, main user flows, and exception handling before technical design or coding.
- The user asks to expand high-level feature notes into a standardized requirement document.

## Standardized Output Format

Always output the document using this exact Markdown structure:

```markdown
# 1. 개요 및 목적 (Objective)

- **목적:** [기능 도입 배경 및 달성하고자 하는 비즈니스/사용자 목표를 1~2문장으로 명확히 서술]
- **적용 범위:** [기능이 영향을 미치는 주요 화면, 프로세스, 시스템 영역 나열]

# 2. 핵심 요구사항 (User Stories)

주체(Actor), 행위(Action), 결과(Outcome) 관점에서 시스템이 제공해야 할 핵심 기능입니다.

| ID | 구분 | 주체 (Actor) | 요구사항 (행위 및 결과) |
| :--- | :--- | :--- | :--- |
| REQ-01 | [기능 구분] | [주체] | [조건/상황]을 통해 [행위]하여 [결과]할 수 있다. |
| REQ-02 | [기능 구분] | [주체] | [조건/상황]을 통해 [행위]하여 [결과]할 수 있다. |

# 3. 상세 정책 및 스펙 (Business Rules)

시나리오 구현 시 반드시 확정되어야 하는 시스템 동작 기준입니다.

- **[정책 항목 1 (예: 식별 기준)]:** [구체적인 기준 및 동작 방식 설명]
- **[정책 항목 2 (예: 상태/분기 판별)]:** [조건에 따른 시스템 판별 로직 설명]
- **[정책 항목 3 (예: 충돌/중복 처리)]:** [데이터 중복이나 계정 충돌 시 처리 정책 설명]
- **[정책 항목 4 (예: 제약 원칙)]:** [연동 비율, 권한 범위 등 핵심 제약 사항 설명]

# 4. 사용자 시나리오 (User Scenarios)

## 시나리오 A: [시나리오명]

1. **[단계명]:** [사용자 진입 또는 트리거 행동]
2. **[단계명]:** [사용자의 구체적 선택/입력 행위]
3. **[단계명]:** [외부 연동 또는 시스템 처리 과정]
4. **[분기 단계명]:**
   - **[Case A-1] [조건 1]:** [해당 조건에서의 화면 전환 및 완료 상태]
   - **[Case A-2] [조건 2]:** [해당 조건에서의 화면 전환 및 완료 상태]

## 시나리오 B: [시나리오명]

1. **[단계명]:** [사용자 진입 또는 트리거 행동]
2. **[단계명]:** [사용자의 구체적 선택/입력 행위]
3. **[단계명]:** [외부 연동 또는 시스템 처리 과정]
4. **[단계명]:** [최종 완료 상태 및 화면 갱신 결과]

# 5. 예외 처리 시나리오 (Edge Cases)

- **[예외 상황 1 (예: 중복/충돌 시도)]:** [발생 조건 및 시스템의 차단/안내 메시지 처리 방식]
- **[예외 상황 2 (예: 사용자 취소/권한 거부)]:** [발생 조건 및 복귀 화면, 안내 처리 방식]
- **[예외 상황 3 (예: 외부 연동/시스템 장애)]:** [발생 조건 및 중단/재시도 유도 처리 방식]
```

## Workflow Steps

1. **Generate Complete Draft Immediately:** Analyze the user's rough notes and immediately produce the full 5-section specification document without asking preliminary questions.
2. **Structure User Stories into a Table:** Extract distinct actors and behaviors from the input and assign sequential IDs (`REQ-01`, `REQ-02`, ...).
3. **Infer Essential Business Rules:** Identify implicit policies required to make the scenarios unambiguous (e.g., identifier keys, first-time user criteria, duplicate handling, 1:1 vs 1:N mapping) and explicitly define them in Section 3.
4. **Detail Step-by-Step Scenarios with Branches:** Write numbered steps with bold action titles. Use `[Case X-Y]` sub-bullets whenever the flow splits based on user state or system conditions.
5. **Anticipate Core Edge Cases:** Always include realistic failure modes in Section 5 covering duplicate/invalid states, user cancellation, and external/network errors.

## Gotchas

- **Missing Business Rules or Edge Cases:** Even if the user's input only mentions happy-path scenarios, do not leave Sections 3 and 5 empty. Proactively infer standard domain rules (like duplicate account conflicts or user cancellation) so the spec is actionable.
- **Overly Low-Level Code/Schema:** Keep policies at the functional/system behavior level (e.g., "고유 식별자(sub) 기준 판별") rather than writing raw SQL queries, DDL schemas, or specific framework code.
- **Inconsistent Formatting:** Maintain the exact section numbering (`# 1.` through `# 5.`), table columns, and `[Case X-Y]` branching syntax across all generated documents.