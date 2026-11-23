# Frontend Architecture & Coding Guidelines

This repository follows a **Vertical Slice / Feature-Based Architecture** built with **React, TypeScript, TanStack (Query & Router), ky, shadcn/ui, and Tailwind CSS v4**.
All AI agents MUST strictly adhere to the directory boundaries, public barrel rules, state management patterns, and styling conventions defined below.

---

## 1. Directory Structure & Dependency Boundaries

```text
src/
├── app/
│   ├── layouts/      # Global shells (DashboardLayout, AuthLayout, PublicLayout)
│   ├── providers/    # Global providers (QueryClientProvider, RouterProvider, Theme)
│   └── routes/       # TanStack Router route tree (params, loaders, layout routes)
├── features/         # Self-contained vertical slices (auth, product, order, etc.)
│   └── <domain>/
│       ├── index.ts  # Public API barrel (exports pages, reusable widgets, queryOptions)
│       ├── api/      # API fetchers, Query Key factories, queryOptions, mutations
│       ├── components/ # Domain-specific UI blocks (layout-agnostic)
│       ├── hooks/    # Domain-specific custom hooks
│       ├── pages/    # Domain screens & cross-feature page assembly
│       └── types/    # Domain TypeScript interfaces & Zod schemas
├── shared/           # Domain-agnostic, globally reusable modules
│   ├── api/          # Base HTTP client (ky), hooks, error classes
│   ├── components/   # Pure UI primitives based on shadcn/ui
│   ├── hooks/        # Generic hooks (useDebounce, useMediaQuery)
│   ├── lib/          # Utilities (cn, date formatters, validators)
│   └── types/        # Global utility types & standard API response wrappers
└── styles/           # Global CSS & Tailwind v4 @theme tokens

```

### Strict Dependency Rules (DO NOT VIOLATE)

1. **Unidirectional Flow:** `app` -> `features` -> `shared`.
2. **Feature Isolation & Public Barrel (`index.ts`):**
* `features/A` internals (`api/`, `components/`, `hooks/`, `types/`) MUST NOT import anything from `features/B`.
* Only `features/A/pages/` and `app/` may import from `features/B`, and **ONLY via its root barrel (`@/features/B`)**. Direct deep imports into `features/B/components/*` or `features/B/api/*` are strictly forbidden.


3. **Pure `shared/`:** Modules inside `shared/` MUST NOT import anything from `features/` or `app/`.

---

## 2. Page Placement & Layer Responsibilities

1. **`features/<domain>/pages/` (All Screens & Composition):**
* Place every screen inside its owning vertical slice (`features/product/pages/ProductListPage.tsx`).
* When a screen requires widgets from another feature (e.g., `OrderDetailPage` embedding a widget from `features/delivery`), assemble them inside `features/order/pages/OrderDetailPage.tsx` by importing from `@/features/delivery` (`index.ts`).
* Pages handle Tailwind `grid`/`flex` layout placement and cross-feature prop wiring; they MUST NOT contain raw API fetch calls.


2. **`features/<domain>/index.ts` (Public Boundary):**
* Explicitly export only the pages, components, `queryOptions`, or types intended to be consumed by `app/` or other feature pages. Keep internal helpers unexported.


3. **`app/routes/` (URL & Data Boundary):**
* Handles URL path/search param validation (via Zod), route guards, and data pre-fetching in `loader`s using `queryClient.ensureQueryData(...)`.
* Keep route files thin: delegate outer shells to `app/layouts/` (`<Outlet />`) and render `features/<domain>/pages/*Page` imported via `@/features/<domain>`.


4. **`features/<domain>/components/` (Self-Contained Blocks):**
* Build components to fill their parent container (`w-full`) without hardcoded outer margins (`m-*`) or viewport positioning. Screen grid placement belongs to `pages/`.



---

## 3. TanStack Query & State Management Rules

Use **ky** for the shared HTTP client in `shared/api/`.

1. **No Raw `useQuery` in Components or Pages:** UI files MUST NOT call `useQuery` or `useMutation` directly with inline `queryFn` or string-array keys.
2. **Query Key Factory & `queryOptions`:** Every domain in `features/<domain>/api/` must define a Query Key Factory and export `queryOptions` (TanStack Query v5 standard) so they can be shared between hooks and route `loader`s.
```ts
// features/product/api/product.queries.ts
export const productKeys = {
  all: ['products'] as const,
  lists: () => [...productKeys.all, 'list'] as const,
  list: (filters: ProductFilter) => [...productKeys.lists(), filters] as const,
  details: () => [...productKeys.all, 'detail'] as const,
  detail: (id: string) => [...productKeys.details(), id] as const,
};

export const productDetailOptions = (id: string) =>
  queryOptions({
    queryKey: productKeys.detail(id),
    queryFn: () => fetchProductDetail(id),
  });

```


3. **Server vs. Client State:**
* All server data belongs in **TanStack Query**. Never copy server response data into `useState` or Zustand unless initializing a local form.
* Use **Zustand** strictly for minimal global client UI state (e.g., sidebar open/close, active modal state).



---

## 4. Forms & Validation

* Use **React Hook Form + Zod** for all forms with validation. Do not write manual `onChange` state handlers for multi-field forms.
* Validate untrusted external API responses and URL search params with **Zod** schemas.

---

## 5. Tailwind CSS v4 & Styling Rules

Use **shadcn/ui** as the foundation for reusable UI components in `shared/components/`.

1. **CSS-First Configuration:** Do NOT create `tailwind.config.js` or `tailwind.config.ts`. Define all design tokens (colors, fonts, spacing) inside `src/styles/globals.css` using `@theme`.
2. **Class Merging:** Always use the `cn()` utility (`clsx` + `tailwind-merge` located in `shared/lib/utils.ts`) when combining conditional classes or accepting `className` props.
3. **Component Variants:** Use `cva` (Class Variance Authority) for components in `shared/components/` that support multiple visual variants or sizes.
4. **No Arbitrary Magic Values:** Prefer theme tokens (`bg-primary`, `text-muted-foreground`) over hardcoded hex codes (`bg-[#3b82f6]`).

---

## 6. TypeScript & Code Quality Guardrails

* **Strict Typing:** Never use `any`. Use `unknown` with type guards or Zod schemas for external/untrusted data.
* **No Premature Abstraction:** Do not create generic layout wrappers (e.g., `<SplitLayout />`) or helper files for single-use logic. Keep code colocated inside the relevant `features/<domain>/` slice until reuse is actually required.
* **File Naming:**
* Components / Pages / Layouts: `PascalCase.tsx` (e.g., `LoginForm.tsx`, `ProductListPage.tsx`, `DashboardLayout.tsx`)
* Hooks: `camelCase.ts` prefixed with `use` (e.g., `useAuthSession.ts`)
* API / Queries / Utils / Types: `kebab-case.ts` or `domain.role.ts` (e.g., `product.queries.ts`, `auth.types.ts`)



---

## 7. Agent Execution & Verification Protocol

1. **Inspect Primitives First:** Before generating a new UI component or utility, inspect `shared/components/` and `shared/lib/` to reuse existing building blocks.
2. **Vertical Slice Containment:** Build features end-to-end (`api/`, `components/`, `pages/`, `index.ts`) inside `features/<target-domain>/`. When composing another feature inside `pages/`, import strictly through `@/features/<other>`.
3. **Verification Gate:** Before concluding a task, run `npm run typecheck` and `npm run lint` from `conbira-ui`, and verify the rendered route works without console errors.
