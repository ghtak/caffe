# Conbira UI Design Guidelines

`conbira-ui`의 목표 디자인 시스템 및 UX 단일 기준(Single Source of Truth).
아키텍처·상태·의존성 경계는 [frontend_structure.md](./frontend_structure.md)를 따르며, 본 문서는 **시각·레이아웃·컴포넌트·상태 표현의 목표 규격**을 정의.
기존 코드의 파편화된 스타일을 추종하지 않고, 본 규격을 기준으로 `globals.css` → `shared/components/ui` → `app/layouts` → `features/*` 순으로 정렬.

---

## 1. 디자인 방향 및 원칙

- **모던 실무형 (Modern Operational)**: 커뮤니티와 관리 콘솔을 하나의 일관된 언어로 통합. 과도한 장식(그라데이션, 과한 그림자) 배제, 명확한 정보 위계와 균형 잡힌 여백 우선.
- **표면 대비 중심의 계층**: 그림자 남발 대신 `background`(캔버스) / `card`(표면) / `muted`(보조 영역) / `border`(1px 경계)의 명도 차로 영역 구분.
- **절제된 포인트**: 브랜드·핵심 CTA·현재 위치 표시에만 `primary`(Indigo/Blue)와 `accent` 집중 사용.
- **단일 출처(SSOT)**: 새 화면·컴포넌트 작성 시 feature 내부에 임의 스타일을 복제하지 않고, 본 규격과 [shared/components/ui](../../conbira-ui/src/shared/components/ui) 조합 사용.

---

## 2. 컬러 & 표면(Surface) 시스템

구현은 [globals.css](../../conbira-ui/src/styles/globals.css)의 `:root` (`light-dark()` + `data-theme`) 및 `@theme inline`에서 단일 관리.

### 2-1. 팔레트 방향 (Neutral Base + Modern Indigo/Blue)

- **베이스(Neutral)**: 라이트 모드에서는 눈 피로를 줄이는 클린 뉴트럴 캔버스 + 순백 카드 표면, 다크 모드에서는 깊이감 있는 다크 슬레이트/차콜 캔버스 + 한 단계 밝은 카드 표면으로 대비 확보.
- **포인트(Indigo/Blue)**: 차분하고 선명한 모던 인디고/블루 계열을 `primary`로 고정. 호버/선택 배경은 채도를 낮춘 `accent` 사용.
- **시맨틱 상태 색**: 오류·삭제(`destructive`), 경고/주의, 성공 상태는 의미 토큰으로만 표현.

### 2-2. 시맨틱 토큰 사용 규약

| 계층/용도 | 필수 토큰 | 금지 사항 |
| --- | --- | --- |
| 앱 전체 배경(Canvas) | `bg-background`, `text-foreground` | `bg-white`, `text-black`, `bg-gray-*` |
| 카드·헤더·사이드바·패널(Surface) | `bg-card`, `text-card-foreground` | 임의 hex/oklch 직접 선언 |
| 보조 배경·테이블 헤더·빈 상태 | `bg-muted`, `bg-muted/50`, `text-muted-foreground` | `text-zinc-*` 등 팔레트 색상 직접 사용 |
| 핵심 액션·브랜드·활성 탭 텍스트 | `bg-primary`, `text-primary`, `text-primary-foreground` | 한 영역 내 primary 버튼 2개 이상 배치 |
| 활성 내비게이션·호버 강조 | `bg-accent`, `text-accent-foreground` | 임의 hover 색 하드코딩 |
| 오류·파괴적 액션 | `text-destructive`, `bg-destructive` | 임의 red 계열 하드코딩 |
| 경계선·입력창·포커스 링 | `border-border`, `border-input`, `outline-ring` | 임의 border/outline 색상 |

- **다크 모드**: `globals.css`의 `light-dark()`와 `ui` primitive 내부 외, feature 페이지에서 `dark:` 조건부 클래스 직접 작성 금지.

---

## 3. 타이포그래피 & 간격 스케일

### 3-1. 타이포그래피 위계

| 역할 | 클래스 규격 | 용도 |
| --- | --- | --- |
| Eyebrow / Breadcrumb | `text-sm font-medium text-primary` | 상단 경로·섹션 식별자 |
| Page Title (`h1`) | `text-2xl font-bold tracking-tight sm:text-3xl` | 모든 화면 최상단 제목 (예외 없음) |
| Section Title (`h2`) | `text-lg font-semibold tracking-tight` | 카드·목록·폼 섹션 제목 |
| Body / Default | `text-sm leading-6 text-foreground` | 본문, 테이블 셀, 일반 UI |
| Caption / Helper | `text-sm text-muted-foreground` (보조 메타·필드 오류는 `text-xs`) | 설명, 타임스탬프, 부가 안내 |

### 3-2. 간격 & 반경 리듬 (4px / 8px 그리드)

- **페이지 컨테이너**: `mx-auto flex w-full max-w-6xl min-w-0 flex-col gap-6 px-4 py-6 sm:px-6 sm:py-8 lg:px-8`. 모든 페이지에 동일 컨테이너 규격 적용.
- **수직 리듬**:
  - 페이지 주요 블록 간: `gap-6` (24px)
  - 섹션 헤더 ↔ 본문, 폼 필드 그룹 간: `gap-4` (16px)
  - 라벨 ↔ 입력창, 인라인 버튼 그룹 간: `gap-2` (8px)
- **카드 패딩**: 기본 `p-6`, 컴팩트 패널 `p-4`.
- **반경(`radius`)**: `--radius: 0.5rem` 기준. 컨트롤(`Button`, `Input`, `Select`) `rounded-md`, 카드·테이블 컨테이너·알림 `rounded-lg`.

---

## 4. 전역 셸(App Shell) & 내비게이션

```text
+-------------------------------------------------------------------+
| Topbar (min-h-14, bg-card, border-b)                              |
| [Brand]   [주요 섹션 링크]              [계정 상태 / Auth / Theme] |
+--------------------+----------------------------------------------+
| Sidebar (w-60,     | Main Content Area (bg-background, flex-1)    |
| bg-card, border-r, |  +----------------------------------------+  |
| md+ 고정 표시)     |  | Page Container (max-w-6xl, gap-6)      |  |
|                    |  |  - PageHeader (Eyebrow + h1 + Desc)    |  |
| - 섹션 내비게이션   |  |  - Toolbar / Filter Bar                |  |
| - 활성 항목 강조    |  |  - Content (Table / Cards / Form)      |  |
|                    |  +----------------------------------------+  |
+--------------------+----------------------------------------------+
```

- **통합 상단 바 (`RootLayout`)**: [app/layouts](../../conbira-ui/src/app/layouts)에서 관리. 높이 `min-h-14`, 좌측 브랜드 + 전역 섹션(`커뮤니티`, 권한 보유 시 `관리`), 우측 인증 액션 및 `ThemeSelect`(`size="sm"`, `h-8`).
- **섹션 사이드바 (`SidebarShell` — `CommunityLayout`, `AdminLayout`)**:
  - `md` 이상: 좌측 `15rem` (`w-60`) 고정 사이드바(`bg-card`, `border-r`).
  - `md` 미만(모바일): 상단 2열 탭 바 전환.
  - 활성 메뉴: `aria-[current=page]:bg-accent aria-[current=page]:font-semibold aria-[current=page]:text-primary`.
- **표준 페이지 헤더 (`PageHeader`)**: 모든 feature 페이지 최상단에 배치. Eyebrow + `h1` + 선택적 설명 + 우측 `actions` 슬롯 정렬.

---

## 5. 컴포넌트 규격 (`shared/components/ui`)

모든 공통 UI primitive는 [shared/components/ui](../../conbira-ui/src/shared/components/ui)에서 `cva` + `cn()`으로 제공.

### 5-1. 컨트롤 높이 & 정렬

- **표준 컨트롤 높이**: 기본 `h-10` (`40px`)로 `Button`, `Input`, `Select` 높이 통일(컴팩트 테이블/상단 바 액션용 `sm: h-8`, 대형 CTA용 `lg: h-11`).
- **네이티브 요소 직접 스타일링 금지**: `<select>`, `<textarea>`, 배지, 페이지네이션 등이 2개 이상 feature에서 필요하면 `shared/components/ui/`에 primitive 추가 후 재사용.

### 5-2. `Button` 사용 위계

| Variant | 용도 | 제한 |
| --- | --- | --- |
| `default` | 화면·폼의 핵심 행동 (저장, 글쓰기, 검색, 로그인) | 영역당 1개 원칙 |
| `secondary` | 보조 강조 액션 | — |
| `outline` | 새로고침, 다시 시도, 취소 등 중립 보조 액션 | — |
| `ghost` | 툴바 아이콘, 로고, 상단 바/인라인 보조 버튼 | — |
| `destructive` | 삭제, 권한 회수 등 되돌릴 수 없는 액션 | 확인 단계 동반 |
| `link` | 문맥 내 텍스트 링크형 액션 | — |

### 5-3. 데이터 테이블 (`Table`) & 필터 바

- **필터/검색 바**: 상단 `flex flex-wrap items-end gap-3` 또는 반응형 `grid` 배치. 검색 입력·필터·제출 버튼 높이(`h-10`) 일치.
- **테이블 컨테이너**: `Table` 기본 컨테이너(`overflow-x-auto rounded-lg border bg-card`)와 `TableHeader`(`bg-muted/50`) 일관 적용. 긴 문자열 컬럼은 `min-w-*` + `whitespace-normal break-words` 처리.

---

## 6. 공통 상태 UI 패턴

| 상태 | 표준 규격 | 접근성 속성 |
| --- | --- | --- |
| **초기 로딩** | Skeleton 블록 또는 `<div role="status" className="rounded-lg border bg-card p-6 text-sm text-muted-foreground">불러오는 중…</div>` | `role="status"` |
| **오류** | `<Alert variant="destructive"><AlertDescription>…</AlertDescription></Alert>` + 필요 시 하단 `outline` 재시도 버튼 | `role="alert"` |
| **빈 결과** | `EmptyState` 컴포넌트(`rounded-lg border border-dashed bg-muted/30 p-8 text-center text-sm text-muted-foreground`) | — |
| **성공·상태 피드백** | `<Alert>` 또는 상태 배너(`role="status"`)로 통일. 임의 `<p>` 텍스트만 노출 금지 | `role="status"` |
| **폼 검증 오류** | 입력창 `aria-invalid` + 필드 하단 `<p role="alert" className="text-xs text-destructive">…</p>` | `role="alert"` |
| **파괴적 액션** | 삭제·권한 변경 등은 즉시 실행 전 명시적 확인 UI(다이얼로그 또는 인라인 확인 바) 제공 | 포커스 관리 |

---

## 7. 접근성 및 UI 카피 규칙

- **시맨틱 랜드마크**: `<header>`, `<nav aria-label="...">`, `<main>`, `<section aria-label="...">` 구조 준수.
- **키보드·포커스**: 모든 인터랙티브 요소에 `focus-visible` 링 유지(`outline-ring/50`). 클릭 가능한 `div` 금지 (`<button>` 또는 `<Link>` 사용).
- **아이콘 전용 요소**: 가시 텍스트가 없는 버튼·링크에는 `aria-label` 필수.
- **한국어 카피 톤**:
  - 제목·라벨·버튼: 간결한 명사형 (`사용자 목록`, `새로고침`, `파일 업로드`).
  - 상태·오류 문구: 짧고 명확한 평서문/청유형 (`목록을 불러오지 못했습니다.`, `다시 시도해 주세요.`).
