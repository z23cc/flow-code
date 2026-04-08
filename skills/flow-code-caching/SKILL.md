---
name: flow-code-caching
description: "Use when adding caching layers — HTTP cache, CDN, Redis, in-memory, or application-level. Covers cache invalidation, TTL strategy, and cache-aside patterns."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: caching,redis,cdn,performance -->

# Caching Strategies

## Overview

Caching is a trade-off: speed vs. staleness. Every cache needs an invalidation strategy before it's built. The two hardest problems in computer science: cache invalidation, naming things, and off-by-one errors.

## When to Use

- API responses are slow and data changes infrequently
- Same data requested by many users (product catalog, config)
- Expensive computations (aggregations, reports, search results)
- Static assets (images, CSS, JS bundles)
- Rate-limited external APIs (cache responses to reduce calls)

**When NOT to use:**
- Data that must be real-time (financial transactions, live chat)
- User-specific data that's rarely re-requested
- Write-heavy workloads (cache invalidation overhead > benefit)

## Cache Layers (Top to Bottom)

```
Browser Cache ──> CDN ──> Reverse Proxy ──> App Cache ──> Database
  (client)      (edge)    (nginx/Varnish)   (Redis)      (source)
```

Each layer closer to the user is faster but harder to invalidate.

## HTTP Caching

```typescript
// Static assets: long cache + content hash in filename
res.setHeader('Cache-Control', 'public, max-age=31536000, immutable');
// File: /assets/app.a1b2c3.js (hash changes on content change)

// API responses: short cache + revalidation
res.setHeader('Cache-Control', 'private, max-age=60, stale-while-revalidate=300');
res.setHeader('ETag', computeETag(data));

// Sensitive data: never cache
res.setHeader('Cache-Control', 'no-store');
```

| Directive | Use For |
|-----------|---------|
| `public, max-age=31536000, immutable` | Hashed static assets (JS, CSS, images) |
| `private, max-age=60` | User-specific API data |
| `public, max-age=300, stale-while-revalidate=3600` | Shared API data (product catalog) |
| `no-store` | Auth tokens, PII, financial data |
| `no-cache` | Always revalidate (ETag/Last-Modified) |

## Application Cache (Redis / In-Memory)

### Cache-Aside Pattern

```typescript
async function getUser(userId: string): Promise<User> {
  // 1. Check cache
  const cached = await redis.get(`user:${userId}`);
  if (cached) return JSON.parse(cached);

  // 2. Cache miss → fetch from DB
  const user = await db.users.findById(userId);
  if (!user) throw new NotFoundError();

  // 3. Populate cache with TTL
  await redis.set(`user:${userId}`, JSON.stringify(user), 'EX', 300);
  return user;
}

// Invalidate on write
async function updateUser(userId: string, data: Partial<User>) {
  await db.users.update(userId, data);
  await redis.del(`user:${userId}`);  // Delete, don't update (simpler)
}
```

### Write-Through Pattern

```typescript
// Write to cache AND database together
async function updateProduct(id: string, data: Partial<Product>) {
  const product = await db.products.update(id, data);
  await redis.set(`product:${id}`, JSON.stringify(product), 'EX', 3600);
  return product;
}
```

## Invalidation Strategies

| Strategy | How | Best For |
|----------|-----|----------|
| **TTL (Time-to-Live)** | Cache expires after N seconds | Data where staleness is acceptable |
| **Event-driven** | Invalidate on write/update event | Data that must be fresh after mutation |
| **Version key** | Append version to cache key | Bulk invalidation (clear all product caches) |
| **Purge on deploy** | Clear cache on new deployment | Config, templates, feature flags |

**Golden Rule:** `cache.del()` on write is simpler and safer than `cache.set()` on write. Let the next read repopulate.

## Cache Key Design

```typescript
// Good: predictable, scoped, versioned
`user:${userId}`
`products:list:page=${page}:sort=${sort}`
`config:v${version}:${tenantId}`

// Bad: unpredictable or unbounded
`data_${Date.now()}`           // Never hit, always miss
`search:${fullQueryString}`    // Unbounded cardinality
```

**Rules:**
- Include all parameters that affect the response
- Use delimiters consistently (`:` for Redis keys)
- Set maximum key count (eviction policy: LRU, LFU)
- Monitor hit rate (< 80% hit rate means cache isn't helping)

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "Cache everything" | Caching data that changes every second wastes memory and adds invalidation complexity. |
| "TTL handles invalidation" | Stale data for the TTL duration may be unacceptable. Use event-driven for critical data. |
| "We'll figure out invalidation later" | Invalidation IS the design. A cache without an invalidation strategy is a bug factory. |
| "In-memory cache is enough" | In-memory caches don't survive restarts and aren't shared across instances. Use Redis for shared state. |

## Red Flags

- Cache without TTL or invalidation strategy
- Caching user-specific data with `public` Cache-Control
- Cache key doesn't include all varying parameters (serving wrong data)
- No cache eviction policy (memory grows unbounded)
- Caching mutable data without write-through or invalidation
- `no-cache` confused with `no-store` (they're different)
- Missing `Vary` header for content negotiation (language, encoding)

## Verification

- [ ] Every cache entry has TTL or explicit invalidation strategy
- [ ] Cache keys include all parameters that affect the response
- [ ] Write operations invalidate affected cache entries
- [ ] Static assets use content-hashed filenames + immutable cache
- [ ] Sensitive data uses `Cache-Control: no-store`
- [ ] Cache hit rate monitored (target > 80%)
- [ ] Eviction policy configured (LRU/LFU, max memory)
- [ ] Graceful degradation when cache is unavailable (fall through to DB)

**See also:** [Performance Checklist](../../references/performance-checklist.md) for broader optimization patterns.
