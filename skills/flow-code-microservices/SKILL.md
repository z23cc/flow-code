---
name: flow-code-microservices
description: "Use when designing service boundaries, inter-service communication, data ownership, or distributed system patterns. Covers service mesh, saga, event-driven, and decomposition strategies."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: microservices,distributed,events,saga,service -->

# Microservices Patterns

## Overview

Microservices are a deployment strategy, not an architecture goal. Start with a modular monolith; extract services when you have clear domain boundaries and team scaling needs. Every service boundary is a network call — design for failure, latency, and eventual consistency.

## When to Use

- Splitting a monolith along domain boundaries
- Designing inter-service communication
- Implementing distributed transactions (saga pattern)
- Setting up event-driven architecture
- Defining data ownership between services

**When NOT to use:**
- Small teams (< 5 developers) — monolith is faster to develop
- Unclear domain boundaries — premature decomposition creates distributed monolith
- Low traffic — microservices add operational complexity without scaling benefit

## Service Decomposition

### Bounded Context (Domain-Driven)

```
E-commerce example:

  Order Service ──── owns: orders, order_items
  │
  ├── Payment Service ──── owns: payments, refunds
  │
  ├── Inventory Service ──── owns: stock_levels, reservations
  │
  ├── User Service ──── owns: users, profiles, auth
  │
  └── Notification Service ──── owns: templates, delivery_log
```

**Rules:**
- Each service owns its data (no shared databases)
- Services communicate through APIs or events (not direct DB access)
- One team per service (Conway's Law alignment)
- Service names are nouns (domain entities), not verbs (actions)

### When to Split

```
Split when:
  - Two teams need to deploy independently
  - A component has fundamentally different scaling needs
  - A domain boundary is clear and stable
  - Data ownership is unambiguous

Don't split when:
  - "It feels too big" (monolith modules work fine)
  - Teams aren't aligned with service boundaries
  - You'd need distributed transactions for common operations
  - The boundary would require synchronous calls between services
```

## Communication Patterns

### Synchronous (REST / gRPC)

```
Use for: Queries that need immediate response (get user, validate payment)
Avoid for: Operations that can be eventual (send email, update analytics)
```

**Rules:**
- Always set timeouts (don't wait forever)
- Use circuit breakers for inter-service calls
- Retry only idempotent operations
- Include correlation ID in all requests

### Asynchronous (Events / Messages)

```
Use for: Notifications, analytics, cross-service data sync
Avoid for: Operations where caller needs immediate result

Patterns:
  Event Notification: "OrderCreated" → other services react
  Event-Carried State Transfer: event contains full data snapshot
  Command: "SendEmail" → targeted service processes
```

```typescript
// Publisher
await eventBus.publish('order.completed', {
  orderId: order.id,
  userId: order.userId,
  total: order.total,
  completedAt: new Date().toISOString(),
});

// Subscriber (Notification Service)
eventBus.subscribe('order.completed', async (event) => {
  await sendOrderConfirmation(event.userId, event.orderId);
});

// Subscriber (Analytics Service)
eventBus.subscribe('order.completed', async (event) => {
  await recordRevenue(event.total, event.completedAt);
});
```

**Rules:**
- Events are facts (past tense): `OrderCompleted`, not `CompleteOrder`
- Commands are requests (imperative): `SendEmail`, `ProcessPayment`
- Consumers must be idempotent (same event processed twice = same result)
- Include event version for schema evolution

## Saga Pattern (Distributed Transactions)

```
Problem: Order requires payment AND inventory reservation.
         Can't use database transaction across services.

Choreography Saga (event-driven):
  1. Order Service: create order (PENDING) → emit OrderCreated
  2. Payment Service: charge card → emit PaymentSucceeded
  3. Inventory Service: reserve stock → emit StockReserved
  4. Order Service: mark CONFIRMED

  Compensation (if Payment fails):
  2b. Payment Service: emit PaymentFailed
  3b. Inventory Service: release reservation
  4b. Order Service: mark CANCELLED

Orchestrator Saga (coordinator):
  1. Saga Coordinator: create order → call Payment → call Inventory
  2. If any step fails: call compensation for completed steps
  3. Coordinator tracks saga state (journal)
```

**Rules:**
- Every step has a compensation action (undo)
- Saga state must be persisted (survives crashes)
- Use idempotency keys (retry-safe)
- Prefer choreography for simple flows, orchestrator for complex ones

## Data Patterns

### Database Per Service

```
Order DB ──── Payment DB ──── User DB
(Postgres)    (Postgres)      (Postgres)

NO shared database. NO cross-service JOINs.
```

If you need data from another service:
1. **API call** (synchronous, for reads)
2. **Event-carried state** (async, maintain local copy)
3. **Materialized view** (CQRS, for complex queries)

### API Gateway

```
Client → API Gateway → Order Service
                     → Payment Service
                     → User Service

Gateway handles:
  - Authentication (one place, not every service)
  - Rate limiting
  - Request routing
  - Response aggregation (combine data from multiple services)
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We need microservices to scale" | Most apps don't need microservices. A well-designed monolith scales to millions of users. |
| "Microservices = modern architecture" | Microservices are a trade-off, not an upgrade. They add network latency, operational complexity, and debugging difficulty. |
| "We'll figure out data ownership later" | Unclear data ownership = distributed monolith. Define boundaries BEFORE splitting. |
| "We can share a database between services" | Shared database = shared coupling. Every schema change affects all services. |
| "Synchronous calls between all services" | If every request requires 5 synchronous inter-service calls, you have a distributed monolith with worse performance. |

## Red Flags

- Shared database between services
- Service A directly reading Service B's tables
- Every request requires synchronous calls to 3+ services
- No circuit breakers on inter-service calls
- Events without schema versioning
- Saga without compensation actions
- Service boundaries that don't align with team boundaries
- "Microservices" that must be deployed together

## Verification

- [ ] Each service owns its data (no shared databases)
- [ ] Communication is API or event-based (no direct DB access)
- [ ] Synchronous calls have timeouts and circuit breakers
- [ ] Async consumers are idempotent
- [ ] Sagas have compensation for every step
- [ ] Events have schema version
- [ ] Service boundaries align with team boundaries
- [ ] Correlation IDs propagated across all service calls
