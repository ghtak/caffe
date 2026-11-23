# Conbira UI

React, TypeScript, TanStack Router, TanStack Query, Tailwind CSS 기반 프런트엔드.

```sh
npm ci
npm run dev
npm run lint
npm run build
```

화면 통합 테스트는 Vitest·React Testing Library에서 실행합니다. Playwright는 Chromium에서 관리자 세션 유지와 보호 화면 접근을 확인합니다. 인증과 관리 API 응답은 MSW에서 대체합니다.

```sh
npm run verify:agent
npx playwright install chromium --only-shell
npm run verify:e2e
```

구조와 코드 작성 기준: `../docs/agents/frontend_structure.md`
