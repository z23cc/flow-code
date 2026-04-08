---
name: flow-code-observability
description: "Use when adding logging, tracing, metrics, or health endpoints. Covers structured logging, distributed tracing, metric collection, and alerting integration."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: observability,logging,tracing,metrics -->

# Observability

## Overview

Make systems debuggable in production. Observability is not logging — it's the ability to ask arbitrary questions about system behavior without deploying new code. Three pillars: logs (what happened), traces (the journey), metrics (the numbers).

## When to Use

- Adding logging to new services or features
- Implementing distributed tracing across services
- Setting up metrics collection (counters, histograms, gauges)
- Adding health check endpoints
- Debugging production issues that need better visibility

**When NOT to use:**
- Local development debugging (use flow-code-debug instead)
- Performance optimization (use flow-code-performance — it consumes observability data)

## Three Pillars

### Structured Logging

```typescript
// Good: structured, queryable
logger.info('order.completed', {
  orderId: order.id,
  userId: order.userId,
  total: order.total,
  items: order.items.length,
  durationMs: Date.now() - startTime,
});

// Bad: unstructured string interpolation
console.log(`Order ${order.id} completed for user ${order.userId} total ${order.total}`);
```

**Rules:**
- Use structured JSON logs (not printf-style strings)
- Include correlation ID in every log (request ID, trace ID)
- Use consistent log levels: `debug` (dev only), `info` (normal operations), `warn` (recoverable issues), `error` (failures requiring attention)
- Never log secrets, tokens, passwords, PII, or full credit card numbers
- Include timing: `durationMs` for operations
- Use consistent field names across services (`userId`, not sometimes `user_id`)

### Distributed Tracing

```typescript
// Propagate trace context across service boundaries
const span = tracer.startSpan('processOrder', {
  attributes: { 'order.id': orderId, 'order.items': itemCount },
});
try {
  await validateOrder(order);      // child span
  await chargePayment(order);      // child span
  await sendConfirmation(order);   // child span
  span.setStatus({ code: SpanStatusCode.OK });
} catch (error) {
  span.setStatus({ code: SpanStatusCode.ERROR, message: error.message });
  throw error;
} finally {
  span.end();
}
```

**Rules:**
- Propagate trace headers (`traceparent` / `X-Request-ID`) across all service calls
- Name spans as `<verb><noun>`: `processOrder`, `validatePayment`, `fetchUser`
- Add key attributes to spans (IDs, counts, status) — not large payloads
- Set span status on error
- Use OpenTelemetry (vendor-neutral) over vendor-specific SDKs

### Metrics

```typescript
// Counter: things that only go up
const requestsTotal = meter.createCounter('http.requests.total', {
  description: 'Total HTTP requests',
});
requestsTotal.add(1, { method: 'GET', path: '/api/orders', status: '200' });

// Histogram: distribution of values (latency, size)
const requestDuration = meter.createHistogram('http.request.duration.ms', {
  description: 'Request duration in milliseconds',
});
requestDuration.record(durationMs, { method, path, status });

// Gauge: current value (queue depth, active connections)
const activeConnections = meter.createUpDownCounter('db.connections.active');
```

**RED Method (request-driven services):**
- **Rate**: requests per second
- **Errors**: error rate (5xx / total)
- **Duration**: latency distribution (p50, p95, p99)

**USE Method (resource-driven infrastructure):**
- **Utilization**: % time resource is busy
- **Saturation**: queue depth / backlog
- **Errors**: error count

## Health Endpoints

```typescript
// Liveness: is the process running?
GET /healthz → 200 { "status": "ok" }

// Readiness: can it serve traffic?
GET /readyz → 200 { "status": "ready", "checks": { "db": "ok", "cache": "ok" } }
             → 503 { "status": "not_ready", "checks": { "db": "ok", "cache": "error" } }
```

- Liveness: simple 200, no dependency checks (used by K8s to restart)
- Readiness: check all dependencies (used by K8s to route traffic)
- Don't put auth on health endpoints
- Include version/build info for debugging

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We'll add logging later" | Production incidents without logs are undiagnosable. Add from day one. |
| "Console.log is fine" | Unstructured logs can't be queried, filtered, or alerted on. |
| "Tracing is overkill for a monolith" | Even monoliths benefit from request-scoped correlation IDs. |
| "We don't need metrics yet" | You can't improve what you can't measure. Basic RED metrics take 30 minutes. |

## Red Flags

- `console.log` with string interpolation in production code
- No correlation/request ID propagated across function calls
- Logging PII, tokens, or full request bodies
- Missing error-level logs in catch blocks
- No health check endpoints
- Metrics with unbounded cardinality (user IDs as labels)
- Log levels all set to `debug` in production

## Verification

- [ ] Structured JSON logging (not string interpolation)
- [ ] Correlation ID present in all log entries for a request
- [ ] No secrets or PII in logs
- [ ] Error paths log at `error` level with context
- [ ] Health endpoints return correct status (liveness + readiness)
- [ ] Key operations have timing metrics (durationMs)
- [ ] Trace context propagated across service boundaries (if multi-service)
