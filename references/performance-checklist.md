# Performance Checklist

Quick reference for `flow-code-performance` and code review. Measure first, optimize second.

## Core Web Vitals Targets

| Metric | Good | Needs Work | Poor |
|--------|------|------------|------|
| **LCP** (Largest Contentful Paint) | <= 2.5s | <= 4.0s | > 4.0s |
| **INP** (Interaction to Next Paint) | <= 200ms | <= 500ms | > 500ms |
| **CLS** (Cumulative Layout Shift) | <= 0.1 | <= 0.25 | > 0.25 |

## Frontend

- [ ] Images: use `<img loading="lazy">`, modern formats (WebP/AVIF), explicit `width`/`height`
- [ ] Fonts: `font-display: swap`, preload critical fonts, subset unused glyphs
- [ ] Bundle: code-split routes, tree-shake unused exports, analyze with `source-map-explorer`
- [ ] CSS: no unused styles in critical path, `content-visibility: auto` for off-screen content
- [ ] Scripts: defer non-critical JS, avoid render-blocking `<script>` tags
- [ ] Caching: `Cache-Control` headers for static assets, service worker for repeat visits
- [ ] Layout: no CLS from dynamic content (reserve space for images, ads, embeds)

## React / Frontend Frameworks

- [ ] No unnecessary re-renders (React DevTools Profiler)
- [ ] `React.memo` only on measured bottlenecks (not everywhere)
- [ ] `useMemo`/`useCallback` only when child components depend on referential equality
- [ ] Lists use stable `key` props (not array index)
- [ ] Large lists use virtualization (`react-window`, `@tanstack/virtual`)
- [ ] Server components / SSR for data-heavy pages

## Backend / API

- [ ] No N+1 queries (use eager loading / `JOIN` / `DataLoader`)
- [ ] List endpoints paginated (no unbounded `SELECT *`)
- [ ] Database queries use indexes (check `EXPLAIN ANALYZE`)
- [ ] Expensive computations cached (Redis, in-memory, HTTP cache headers)
- [ ] Async operations don't block request thread (use queues for heavy work)
- [ ] Connection pooling configured (DB, HTTP clients)
- [ ] Response compression enabled (gzip/brotli)

## Monitoring

- [ ] Real User Monitoring (RUM) collecting Core Web Vitals
- [ ] Server-side latency tracked (p50, p95, p99)
- [ ] Error rate dashboards with alerting
- [ ] Performance budget defined and enforced in CI

## Testing Tools

```bash
# Lighthouse CI
npx lhci autorun

# Web Vitals library (in-app)
import { onLCP, onINP, onCLS } from 'web-vitals';

# Bundle analysis
npx source-map-explorer dist/**/*.js

# Database query analysis
EXPLAIN ANALYZE SELECT ...;
```

## Anti-Patterns to Flag in Review

- `SELECT *` without LIMIT
- Synchronous file I/O in request handlers
- Missing pagination on list endpoints
- Image without width/height (causes CLS)
- `JSON.parse` on megabyte-scale strings in hot path
- Polling when WebSocket/SSE would work
- Loading entire library for one utility function
