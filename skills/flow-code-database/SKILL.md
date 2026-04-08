---
name: flow-code-database
description: "Use when designing schemas, writing migrations, optimizing queries, or working with ORMs. Covers migration safety, indexing strategy, and query optimization."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: database,migration,orm,query,schema -->

# Database & ORM Patterns

## Overview

Design schemas for evolution, write migrations that don't break production, and optimize queries based on access patterns. Databases are the hardest part to change later — get the foundation right.

## When to Use

- Designing new tables or modifying schema
- Writing database migrations
- Optimizing slow queries
- Choosing indexes
- Working with ORMs (Prisma, SQLAlchemy, Diesel, TypeORM, etc.)

**When NOT to use:**
- In-memory data structures (use flow-code-api-design)
- Caching layer design (use flow-code-caching)

## Schema Design

### Naming Conventions

- Tables: plural, snake_case (`users`, `order_items`)
- Columns: snake_case, descriptive (`created_at`, not `ts` or `date`)
- Foreign keys: `<singular_table>_id` (`user_id`, `order_id`)
- Booleans: prefix with `is_` or `has_` (`is_active`, `has_verified_email`)
- Timestamps: suffix with `_at` (`created_at`, `updated_at`, `deleted_at`)

### Required Columns

Every table should have:
```sql
id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
```

### Soft Deletes vs Hard Deletes

```
Use soft deletes (deleted_at column) when:
  - Data has legal retention requirements
  - Users might want to recover data
  - Foreign keys reference this table from many places

Use hard deletes when:
  - Data is ephemeral (sessions, tokens, temp files)
  - GDPR/privacy requires actual removal
  - Table is append-only log (delete old entries)
```

## Migration Safety

### Safe Migrations (zero-downtime)

```
SAFE:
  - Add column with DEFAULT or NULL
  - Add index CONCURRENTLY
  - Add new table
  - Add column then backfill then add NOT NULL constraint (3 steps)

UNSAFE (requires maintenance window):
  - Add NOT NULL column without default
  - Drop column that application reads
  - Rename column or table
  - Change column type
  - Add unique constraint without checking data
```

### Safe Migration Pattern (column rename)

```sql
-- Step 1: Add new column
ALTER TABLE users ADD COLUMN display_name TEXT;

-- Step 2: Backfill (deploy code that writes to both)
UPDATE users SET display_name = name WHERE display_name IS NULL;

-- Step 3: Deploy code reading from new column

-- Step 4: Drop old column (next release)
ALTER TABLE users DROP COLUMN name;
```

### Migration Rules

- One migration per change (don't batch unrelated changes)
- Always write a rollback (`down` migration)
- Test migration on a copy of production data size
- Never modify a migration that's been applied to any environment
- Use `CREATE INDEX CONCURRENTLY` to avoid table locks

## Query Optimization

### N+1 Query Detection

```typescript
// BAD: N+1 — one query per user
const orders = await db.orders.findAll();
for (const order of orders) {
  order.user = await db.users.findById(order.userId); // N queries!
}

// GOOD: eager loading — one query
const orders = await db.orders.findAll({
  include: [{ model: User }], // JOIN or second query with IN clause
});
```

### Index Strategy

```
Index when:
  - Column appears in WHERE clauses frequently
  - Column is used in JOIN conditions
  - Column is used in ORDER BY
  - Foreign key columns (always index these)

Don't index when:
  - Table has < 1000 rows (sequential scan is faster)
  - Column has very low cardinality (boolean — only 2 values)
  - Column is frequently updated (index maintenance overhead)
  - Write-heavy table with rare reads
```

### EXPLAIN ANALYZE

Always check query plans for slow queries:
```sql
EXPLAIN ANALYZE SELECT * FROM orders WHERE user_id = 123 AND status = 'pending';

-- Look for:
--   Seq Scan (bad on large tables — needs index)
--   Nested Loop (fine for small sets, bad for large joins)
--   Sort (bad if not using index — consider ORDER BY index)
--   actual time vs estimated rows (large mismatch = stale statistics)
```

### Pagination

Never use OFFSET for deep pagination:
```sql
-- BAD: OFFSET scans and discards rows
SELECT * FROM orders ORDER BY id LIMIT 20 OFFSET 10000;

-- GOOD: cursor-based pagination
SELECT * FROM orders WHERE id > :last_seen_id ORDER BY id LIMIT 20;
```

## ORM Best Practices

- Enable query logging in development (see actual SQL generated)
- Use raw SQL for complex queries (don't fight the ORM)
- Batch operations for bulk inserts/updates
- Use transactions for multi-table writes
- Avoid lazy loading in loops (N+1 trap)
- Define explicit column selection (don't `SELECT *` in production)

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We can add indexes later" | Slow queries in production cause cascading failures. Design indexes with the schema. |
| "The ORM handles it" | ORMs generate SQL. Bad ORM usage generates bad SQL. Check EXPLAIN. |
| "We'll migrate the data later" | Data migrations are the hardest kind. Design the schema for evolution from day one. |
| "This migration is simple, no rollback needed" | Every migration needs a rollback. The one time you skip it is the one time you need it. |
| "OFFSET pagination works fine" | It works until page 500, then it's slower than the query itself. Use cursors. |

## Red Flags

- `SELECT *` in production queries
- Missing indexes on foreign key columns
- N+1 queries in loops (ORM lazy loading)
- Migrations without rollback (`down`) scripts
- NOT NULL added without DEFAULT on existing table
- OFFSET-based pagination on large tables
- No `created_at`/`updated_at` on business tables
- Column renames in a single migration (unsafe)

## Verification

- [ ] Schema follows naming conventions (snake_case, plural tables)
- [ ] All tables have `id`, `created_at`, `updated_at`
- [ ] Foreign key columns have indexes
- [ ] Migrations are reversible (down migration exists)
- [ ] No N+1 queries (check ORM query log)
- [ ] Slow queries checked with EXPLAIN ANALYZE
- [ ] Pagination uses cursor-based approach (not OFFSET)
- [ ] Migrations tested against production-like data volume
