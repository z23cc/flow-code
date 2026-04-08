---
name: flow-code-state-management
description: "Use when designing state architecture for frontend or full-stack apps. Covers state classification, tool selection, server state patterns, and common pitfalls."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: state,frontend,react,store -->

# State Management

## Overview

Choose the simplest state tool that solves the problem. State management is not a framework choice — it's a classification problem. Identify the type of state, then pick the minimal tool. Over-engineering state is the #1 source of unnecessary complexity in frontend apps.

## When to Use

- Designing state architecture for a new app or feature
- Choosing between state management approaches
- Refactoring state that has become tangled
- Adding server state (API data) to the frontend
- Debugging state-related bugs (stale data, unnecessary re-renders)

## State Classification

Classify every piece of state before choosing a tool:

| Type | Lifetime | Examples | Best Tool |
|------|----------|---------|-----------|
| **UI state** | Component | Is dropdown open? Which tab is active? | `useState` |
| **Form state** | Component/page | Input values, validation errors | `useState` or form library |
| **Shared UI** | 2-3 components | Sidebar collapsed? Modal open? | Lift state or Context |
| **Theme/locale** | App-wide, read-heavy | Dark mode, language, auth user | React Context |
| **URL state** | Shareable, bookmarkable | Filters, pagination, search query | URL params (`useSearchParams`) |
| **Server state** | Remote, cached | API data, user profile, order list | React Query / SWR / TanStack Query |
| **Global client** | App-wide, write-heavy | Shopping cart, multi-step wizard | Zustand / Redux / Jotai |

## The Decision Ladder

```
Can it live in one component?
  YES → useState
  NO ↓

Is it shared between 2-3 siblings?
  YES → Lift state to parent
  NO ↓

Is it read-heavy, write-rare (theme, locale, auth)?
  YES → React Context
  NO ↓

Is it URL-representable (filters, pagination, search)?
  YES → URL searchParams
  NO ↓

Does it come from an API?
  YES → React Query / SWR (server state)
  NO ↓

Is it complex client-side state shared across the app?
  YES → Zustand (simple) or Redux (complex with middleware)
```

## Server State (React Query / SWR)

Server state is NOT client state. It's a cached snapshot of remote data. Treat it differently:

```typescript
// React Query: fetch + cache + background refresh
function useOrders() {
  return useQuery({
    queryKey: ['orders'],
    queryFn: () => api.getOrders(),
    staleTime: 5 * 60 * 1000,    // Fresh for 5 minutes
    gcTime: 30 * 60 * 1000,      // Keep in cache for 30 minutes
  });
}

// Mutation with optimistic update
function useToggleOrder() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.toggleOrder(id),
    onMutate: async (id) => {
      await queryClient.cancelQueries({ queryKey: ['orders'] });
      const previous = queryClient.getQueryData(['orders']);
      queryClient.setQueryData(['orders'], (old: Order[]) =>
        old.map(o => o.id === id ? { ...o, done: !o.done } : o)
      );
      return { previous };
    },
    onError: (_err, _id, context) => {
      queryClient.setQueryData(['orders'], context?.previous);
    },
  });
}
```

**Rules:**
- Never duplicate server data into a global store (single source of truth = cache)
- Use `staleTime` > 0 to prevent unnecessary refetches
- Optimistic updates for perceived speed on user actions
- Invalidate related queries after mutations

## Context Pitfalls

```typescript
// BAD: everything in one context (every write re-renders all consumers)
const AppContext = createContext({ user, theme, cart, notifications });

// GOOD: split by update frequency
const AuthContext = createContext({ user });       // Rarely changes
const ThemeContext = createContext({ theme });      // Rarely changes
const CartContext = createContext({ cart });        // Changes often — separate
```

**Rules:**
- Split contexts by update frequency
- Don't put rapidly-changing state in Context (use Zustand/Redux instead)
- Context is for read-heavy, write-rare data
- If more than 5 components consume AND write to the same context, it's time for a store

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We need Redux for everything" | Redux is for complex client state. Most apps need React Query + useState. |
| "Global state is simpler" | Global state couples components and causes mystery re-renders. Start local. |
| "We'll refactor state later" | State architecture is the hardest thing to change. Get classification right early. |
| "Context is fine for everything" | Context re-renders all consumers on every write. Fine for theme, terrible for cart. |
| "We need a store for API data" | API data is server state. React Query/SWR manages it better than any store. |

## Red Flags

- Redux/Zustand store containing API response data (use React Query instead)
- Everything in one React Context (split by update frequency)
- `useEffect` + `useState` for data fetching (use React Query/SWR)
- Prop drilling past 3 levels (restructure or use context)
- Duplicated state (same data in store AND component AND URL)
- Global state for component-local UI (dropdown open, modal visible)
- Missing loading/error states for async data

## Verification

- [ ] Every piece of state classified (UI/form/shared/server/global)
- [ ] Simplest tool chosen per classification (decision ladder followed)
- [ ] Server state uses React Query/SWR (not Redux/Zustand)
- [ ] URL state for shareable/bookmarkable values (filters, pagination)
- [ ] No prop drilling beyond 3 levels
- [ ] Contexts split by update frequency
- [ ] Loading, error, and empty states handled for all async data
- [ ] No duplicated state across multiple sources
