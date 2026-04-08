---
name: flow-code-error-handling
description: "Use when designing error handling strategy, implementing retry logic, circuit breakers, or graceful degradation. Covers error classification, recovery patterns, and user-facing error UX."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: errors,resilience,retry,recovery -->

# Error Handling Patterns

## Overview

Errors are not exceptional — they're expected. Design for failure from the start. Classify errors, handle them at the right layer, and give users actionable feedback. Never swallow errors silently.

## When to Use

- Designing error handling for new services or features
- Adding retry logic for external API calls
- Implementing circuit breakers for unreliable dependencies
- Improving user-facing error messages
- Adding graceful degradation for non-critical features

## Error Classification

| Type | Retryable? | Example | Strategy |
|------|-----------|---------|----------|
| **Validation** | No | Invalid email, missing field | Return 400 with field-level errors |
| **Not Found** | No | Resource doesn't exist | Return 404, don't retry |
| **Auth** | No | Invalid token, forbidden | Return 401/403, redirect to login |
| **Conflict** | Maybe | Duplicate entry, stale update | Return 409, let client resolve |
| **Rate Limit** | Yes | Too many requests | Return 429, retry after backoff |
| **Server Error** | Yes | Internal failure, timeout | Return 500, retry with backoff |
| **Network** | Yes | Connection refused, DNS failure | Retry with exponential backoff |
| **Dependency** | Yes | Third-party API down | Circuit breaker + fallback |

## Error Response Format

Pick one format, use everywhere:

```typescript
interface ErrorResponse {
  code: string;        // Machine-readable: "VALIDATION_ERROR", "NOT_FOUND"
  message: string;     // Human-readable: "Email is required"
  details?: {          // Optional structured context
    field?: string;    // Which field failed
    reason?: string;   // Why it failed
  }[];
}

// Example: validation error
{
  "code": "VALIDATION_ERROR",
  "message": "Invalid input",
  "details": [
    { "field": "email", "reason": "Must be a valid email address" },
    { "field": "age", "reason": "Must be at least 13" }
  ]
}
```

## Retry with Exponential Backoff

```typescript
async function withRetry<T>(
  fn: () => Promise<T>,
  { maxRetries = 3, baseDelayMs = 200, maxDelayMs = 10000 } = {}
): Promise<T> {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await fn();
    } catch (error) {
      if (!isRetryable(error) || attempt === maxRetries) throw error;
      const delay = Math.min(baseDelayMs * 2 ** attempt, maxDelayMs);
      const jitter = delay * (0.5 + Math.random() * 0.5);
      await sleep(jitter);
    }
  }
  throw new Error('Unreachable');
}

function isRetryable(error: unknown): boolean {
  if (error instanceof HttpError) {
    return [408, 429, 500, 502, 503, 504].includes(error.status);
  }
  return error instanceof NetworkError;
}
```

**Rules:**
- Always add jitter (prevent thundering herd)
- Cap max delay (don't wait forever)
- Only retry idempotent operations (GET, PUT, DELETE) — never POST without idempotency key
- Log each retry attempt with attempt number

## Circuit Breaker

```
States: CLOSED → OPEN → HALF_OPEN → CLOSED

CLOSED (normal): requests pass through
  → failure count exceeds threshold → switch to OPEN

OPEN (blocking): all requests fail immediately (no network call)
  → after timeout period → switch to HALF_OPEN

HALF_OPEN (testing): allow one request through
  → success → switch to CLOSED
  → failure → switch back to OPEN
```

Use when: calling external APIs, unreliable databases, third-party services.
Don't use for: local operations, in-process calls, fast-failing validation.

## Graceful Degradation

```typescript
async function getProductRecommendations(userId: string): Promise<Product[]> {
  try {
    return await recommendationService.getPersonalized(userId);
  } catch (error) {
    logger.warn('recommendation.fallback', { userId, error: error.message });
    return await getPopularProducts(); // Fallback to non-personalized
  }
}
```

**Tiers:**
1. **Full feature**: personalized recommendations from ML service
2. **Degraded**: popular items from cache (ML service down)
3. **Minimal**: static "featured" list (cache also down)
4. **Unavailable**: hide the section entirely (all backends down)

## User-Facing Errors

```typescript
// Bad: technical error exposed to user
"Error: ECONNREFUSED 10.0.0.5:5432"

// Good: actionable message
"We're having trouble loading your orders. Please try again in a few minutes."

// Best: actionable + specific
"Unable to save your changes — another user updated this record. 
 Please refresh and try again."
```

**Rules:**
- Never expose stack traces, internal IPs, or database errors to users
- Provide actionable next steps ("try again", "contact support", "refresh")
- Distinguish permanent failures ("not found") from temporary ("try again later")
- Log the technical details server-side, show the human message client-side

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "This API never fails" | Every external dependency fails eventually. Handle it now. |
| "We'll add error handling later" | Missing error handling IS the bug. It's not a separate task. |
| "Just catch and log it" | Catching without recovery or user feedback = silently broken feature. |
| "Retry will fix it" | Retrying non-idempotent operations can cause duplicate charges, duplicate records. |
| "Users don't need details" | Users need to know what happened and what to do next. Generic "error" is useless. |

## Red Flags

- Empty catch blocks (`catch (e) {}`)
- Generic error messages ("Something went wrong")
- Stack traces in user-facing responses
- Retrying non-idempotent operations
- No timeout on HTTP/DB calls (unbounded wait)
- Circuit breaker missing for external dependencies
- Inconsistent error format across endpoints
- Swallowing errors with `|| null` or `?? default` without logging

## Verification

- [ ] Errors classified (validation/auth/not-found/server/network/dependency)
- [ ] Consistent error response format across all endpoints
- [ ] Retries use exponential backoff with jitter (idempotent operations only)
- [ ] External dependencies have circuit breakers or timeouts
- [ ] User-facing errors are actionable (no stack traces, no internal details)
- [ ] Error paths logged with context (correlation ID, operation, parameters)
- [ ] Graceful degradation for non-critical features
- [ ] No empty catch blocks
