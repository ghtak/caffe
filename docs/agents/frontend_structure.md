# Frontend Architecture & Coding Guidelines

This repository follows a **Feature-Based Architecture** built with **React, TypeScript, TanStack (Query & Router), ky, shadcn/ui, and Tailwind CSS v4**.
All AI agents MUST strictly adhere to the directory boundaries, state management rules, and styling conventions defined below.

---

## 1. Directory Structure & Dependency Boundaries

```text
src/
├── app/              # App entry, global providers, TanStack Router tree
├── features/         # Business domains (auth, product, order, etc.)
│   └── <domain>/
│       ├── api/      # API fetchers, Query Key factories, queryOptions, mutations
│       ├── components/ # Domain-specific UI components
│       ├── hooks/    # Domain-specific custom hooks
│       └── types/    # Domain TypeScript interfaces/schemas
├── shared/           # Domain-agnostic, globally reusable modules
│   ├── api/          # Base HTTP client (ky), hooks, error classes
│   ├── components/   # Reusable UI primitives based on shadcn/ui
│   ├── hooks/        # Generic hooks (useDebounce, useMediaQuery)
│   ├── lib/          # Utilities (cn, date formatters, validators)
│   └── types/        # Global utility types & standard API response wrappers
└── styles/           # Global CSS & Tailwind v4 @theme tokens

```

### Strict Dependency Rules (DO NOT VIOLATE)

1. **Unidirectional Flow:** `app` -> `features` -> `shared`.
2. **No Cross-Feature Imports:** `features/A` MUST NOT directly import internal files from `features/B`. If two features need to share logic or UI, lift it to `shared/` or compose them at the `app/` (route/page) level.
3. **Pure `shared/`:** Modules inside `shared/` MUST NOT import anything from `features/` or `app/`.

---

## 2. TanStack Query & Data Fetching Rules

Use **ky** for the shared HTTP client in `shared/api/`.

1. **No Raw `useQuery` in Components:** Components MUST NOT call `useQuery` or `useMutation` directly with inline `queryFn` or string-array keys.
2. **Query Key Factory & `queryOptions`:** Every domain in `features/<domain>/api/` must define a Query Key Factory and export `queryOptions` (TanStack Query v5 standard) so they can be shared between components and route loaders.
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

## 3. Routing & Forms

1. **TanStack Router:** Define route search params and path params with strict TypeScript/Zod validation. Pre-fetch critical data in route `loader`s using `queryClient.ensureQueryData(productDetailOptions(id))`.
2. **Forms:** Use **React Hook Form + Zod** for all forms with validation. Do not write manual `onChange` state handlers for multi-field forms.

---

## 4. Tailwind CSS v4 & Styling Rules

Use **shadcn/ui** as the foundation for reusable UI components in `shared/components/`.

1. **CSS-First Configuration:** Do NOT create `tailwind.config.js` or `tailwind.config.ts`. Define all design tokens (colors, fonts, spacing) inside `src/styles/globals.css` using `@theme`.
2. **Class Merging:** Always use the `cn()` utility (`clsx` + `tailwind-merge` located in `shared/lib/utils.ts`) when combining conditional classes or accepting `className` props.
3. **Component Variants:** Use `cva` (Class Variance Authority) for components in `shared/components/` that support multiple visual variants or sizes.
4. **No Arbitrary Magic Values:** Prefer theme tokens (`bg-primary`, `text-muted-foreground`) over hardcoded hex codes (`bg-[#3b82f6]`).

---

## 5. TypeScript & Code Quality Guardrails

* **Strict Typing:** Never use `any`. Use `unknown` with type guards or Zod schemas for external/untrusted data.
* **No Premature Abstraction:** Do not create generic wrappers, base classes, or helper files for single-use logic. Keep code colocated inside the relevant `features/<domain>/` folder until reuse is actually required.
* **File Naming:**
* Components: `PascalCase.tsx` (e.g., `LoginForm.tsx`)
* Hooks: `camelCase.ts` prefixed with `use` (e.g., `useAuthSession.ts`)
* API / Queries / Utils / Types: `kebab-case.ts` or `domain.role.ts` (e.g., `product.queries.ts`, `auth.types.ts`)



---

## 6. Agent Execution Protocol

1. **Check Existing Primitives First:** Before generating a new UI component or utility, inspect `shared/components/` and `shared/lib/` to reuse existing building blocks.
2. **Scope Containment:** When implementing a feature, limit file creation/modification strictly to `features/<target-domain>/` and the corresponding route in `app/routes/` unless shared primitives are explicitly requested.
3. **Verification:** Before concluding a task, ensure there are no TypeScript errors (`tsc --noEmit`), broken imports across feature boundaries, or unused variables.
