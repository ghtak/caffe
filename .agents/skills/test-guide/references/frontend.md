# Frontend Agent Testing Harness & Guidelines

이 문서는 **Vite + React + TypeScript + TanStack (Router/Query) + Tailwind** 스택에서 AI 에이전트가 코드를 작성하고 자가 검증(Self-healing)할 때 반드시 준수해야 하는 테스트 아키텍처 및 작성 지침입니다.

---

## 1. Core Philosophy (핵심 원칙)

1. **결정론적 피드백 루프:** 모든 테스트는 외부 인프라나 실행 타이밍에 의존하지 않고 100% 동일한 결과를 보장해야 합니다.
2. **구현이 아닌 행위와 계약 검증:** 내부 컴포넌트 구조나 훅 호출 여부를 테스트하지 않고, **사용자 관점의 UI 변화**와 **네트워크 경계(HTTP Payload)**만 검증합니다.
3. **테스트 비대화(Bloat) 방지:** 모든 케이스를 테스트하지 않으며, 정해진 3-Tier 권위(Contract / Edge / Regression) 내에서 최소한의 핵심 테스트만 작성합니다.

---

## 2. Test Boundaries & Layering (테스트 경계 및 계층)

| 계층 | 도구 | 테스트 경계 (Boundary) | 비중 | 검증 대상 및 역할 |
| :--- | :--- | :--- | :---: | :--- |
| **Static / Contract** | `tsc --noEmit`, ESLint | **타입 & 스키마 경계** | 필수 | Zod 스키마와 TypeScript 타입을 SSOT로 삼아 런타임 이전에 API 계약 위반 차단 |
| **Pure Logic (Unit)** | Vitest | **상태 전이 & 순수 함수** | ~10% | UI와 분리된 순수 데이터 변환, 복잡한 Zod 스키마, 상태 머신 로직만 격리 테스트 |
| **Feature Integration** | Vitest + RTL + **MSW** | **기능 슬라이스 + 네트워크 경계** | **~75%** | **주력 테스트 계층.** 라우트/페이지 단위로 렌더링하고 MSW로 HTTP 응답을 제어해 시나리오 검증 |
| **E2E (Critical Path)** | Playwright | **실제 브라우저 런타임** | **~15%** | 인증 세션 유지(쿠키/JWT), 브라우저 히스토리/Search Params, 파일 업/다운로드 등 핵심 경로만 검증 |

---

## 3. Hard Guardrails (엄격한 금지 사항)

에이전트는 테스트 작성 시 다음 행위를 절대 수행해서는 안 됩니다.

* **내부 구현 모킹 금지:** `vi.mock()`으로 `@tanstack/react-query`, `@tanstack/react-router`, 내부 커스텀 훅(`use*`), API 클라이언트 함수를 모킹하지 않습니다. 외부 경계는 오직 **MSW**로만 통제합니다.
* **원자(Atom) 단위 테스트 금지:** `src/components/ui/` 하위의 기본 UI 컴포넌트(`Button`, `Input`, `Dialog` 등)에 대한 개별 테스트 파일을 생성하지 않습니다.
* **DOM/CSS 셀렉터 금지:** `container.querySelector()`, `.className`, 태그명 기반 탐색을 금지합니다. 오직 ARIA 시맨틱 쿼리(`getByRole`, `getByLabelText`, `getByText`)만 사용하며, `data-testid`는 시맨틱 역할이 없는 경우에만 제한적으로 허용합니다.
* **임의의 대기 시간 금지:** `setTimeout`이나 `{ timeout: 5000 }` 같은 임의의 대기를 사용하지 않습니다. 비동기 UI는 `findBy*` 쿼리를 사용하고, 시간 제어는 `vi.useFakeTimers()`를 사용합니다.
* **E2E 남용 금지:** 폼 유효성 검사 에러, 모달 토글, 단순 API 에러 토스트 출력은 Playwright가 아닌 Vitest + MSW 통합 테스트에서 검증합니다.

---

## 4. Vitest + MSW 작성 규칙 (Feature Integration)

### 4.1 파일 위치 및 구조
* 기능 슬라이스 디렉토리 내 페이지 또는 주요 컨테이너 옆에 `<FeatureName>.test.tsx` 형태로 배치합니다.
* 테스트 격리를 위해 제공되는 `renderWithProviders` 헬퍼(독립 `QueryClient` + `MemoryHistory` 라우터 포함)를 사용합니다.
* 모든 MSW 응답 모킹 데이터는 해당 도메인의 **Zod 스키마**를 만족해야 합니다.

### 4.2 3-Tier 테스트 분류 (Anti-Bloat)
모든 `describe` 블록은 아래 3가지 분류로만 구성합니다.

1. **`contract` (Happy Path):** 정상 입력 → API 요청 페이로드 검증 → `200/201` 응답 후 UI 업데이트(목록 갱신, 성공 메시지 등) 검증. (주요 액션당 1~2개)
2. **`edge` (Boundary & Failure):** 빈 데이터(`[]`) 폴백 UI, 유효성 검증 실패(`400/422`), 인증 만료(`401`), 서버 장애(`500`) 시 에러 알림 UI 검증.
3. **`regression` (Bug Reproduction):** 버그 수정 작업 시에만 추가하며, 재현 조건과 수정된 동작을 검증.

### 4.3 코드 템플릿 (Exemplar)
```tsx
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { server } from '@/test/msw/server';
import { renderWithProviders } from '@/test/renderWithProviders';
import { OrderCreatePage } from './OrderCreatePage';

describe('OrderCreatePage', () => {
  describe('contract', () => {
    it('submits valid order payload and displays confirmation', async () => {
      const user = userEvent.setup();
      let capturedBody: unknown;

      server.use(
        http.post('/api/v1/orders', async ({ request }) => {
          capturedBody = await request.json();
          return HttpResponse.json({ id: 'ord_123', status: 'CREATED' }, { status: 201 });
        })
      );

      renderWithProviders(<OrderCreatePage />);

      await user.type(screen.getByLabelText(/수량/i), '3');
      await user.click(screen.getByRole('button', { name: /주문 생성/i }));

      expect(await screen.findByText(/주문이 접수되었습니다/i)).toBeInTheDocument();
      expect(capturedBody).toMatchObject({ quantity: 3 });
    });
  });

  describe('edge', () => {
    it('renders error alert when server returns 500', async () => {
      const user = userEvent.setup();

      server.use(
        http.post('/api/v1/orders', () =>
          HttpResponse.json({ message: 'Internal Error' }, { status: 500 })
        )
      );

      renderWithProviders(<OrderCreatePage />);

      await user.type(screen.getByLabelText(/수량/i), '3');
      await user.click(screen.getByRole('button', { name: /주문 생성/i }));

      expect(await screen.findByRole('alert')).toHaveTextContent(/처리 중 오류가 발생했습니다/i);
    });
  });
});
```

---

## 5. Playwright 작성 규칙 (E2E Critical Path)

Playwright 테스트는 `e2e/` 디렉토리에 위치하며, jsdom 환경에서 검증 불가능한 항목만 작성합니다.

1. **작성 허용 범위:**
   * 로그인 → HttpOnly 쿠키/토큰 저장 → 페이지 새로고침 후 세션 유지 및 보호된 라우트 접근 제어
   * TanStack Router의 URL Search Params 동기화 및 브라우저 뒤로가기/앞으로가기 히스토리 검증
   * 실제 브라우저 API 의존 기능 (파일 업로드/다운로드, 뷰포트 반응형 레이아웃, 스크롤 이벤트)
2. **데이터 격리:**
   * 인증 상태는 `auth.setup.ts`의 `storageState`를 재사용하여 불필요한 로그인 반복을 피합니다.
   * E2E에서 생성하는 리소스 식별자에는 반드시 `e2e-${crypto.randomUUID()}` 접두사를 사용해 병렬 실행 충돌을 방지합니다.
3. **Flaky 방지:**
   * `retries`는 `0`으로 유지하여 불안정한 테스트를 숨기지 않고 즉시 수정합니다.

---

## 6. Verification Gate & Failure Protocol (검증 실행 및 실패 대응)

작업 완료 전 `conbira-ui`에서 아래 검증 명령을 순서대로 실행합니다.

### 6.1 검증 커맨드
```bash
# 1. 개발 루프 중 수시 실행 (정적 타입 + 린트 + Vitest 통합 테스트)
npm run verify:agent

# 2. 기능 구현 완료 후 최종 관문 (Playwright 핵심 경로 검증)
npm run verify:e2e
```

### 6.2 실패 시 트리아지(Triage) 순서
테스트 실패 시 에이전트는 다음 순서대로 원인을 해결합니다.

1. **`tsc` 타입 에러:** API 응답/요청 스키마(Zod)와 컴포넌트 Props의 타입 불일치를 가장 먼저 수정합니다.
2. **ESLint 오류:** 미사용 변수, 잘못된 훅 의존성, 의존 경계 위반을 수정합니다.
3. **`Vitest / Playwright` 런타임 실패:**
   * 실패 로그에 출력된 ARIA 트리를 확인하여 실제 렌더링된 접근성 역할(`role`)과 라벨이 명세와 일치하는지 점검합니다.
   * MSW 핸들러의 HTTP 메서드/경로가 실제 요청 URL과 일치하는지 확인합니다.
   * **절대 테스트 통과를 위해 단언문(Assertion)을 삭제하거나 `any` 타입을 추가하여 우회하지 않습니다.**
