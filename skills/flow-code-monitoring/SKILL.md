---
name: flow-code-monitoring
description: "Use when setting up dashboards, SLOs/SLIs, alerting rules, or on-call runbooks. Covers the monitoring hierarchy from health checks to incident response."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: monitoring,alerting,slo,dashboard,oncall -->

# Monitoring & Alerting

## Overview

Monitoring answers "is the system healthy?" Alerting answers "does someone need to act?" Good monitoring detects problems before users report them. Good alerting wakes people only when necessary.

## When to Use

- Setting up monitoring for a new service
- Defining SLOs (Service Level Objectives)
- Creating dashboards
- Configuring alerting rules
- Writing on-call runbooks

## SLO/SLI/SLA Framework

| Term | Definition | Example |
|------|-----------|---------|
| **SLI** (Indicator) | Metric that measures service health | 99.2% of requests < 300ms |
| **SLO** (Objective) | Target for the SLI | 99.5% of requests < 300ms over 30 days |
| **SLA** (Agreement) | Business contract with consequences | 99.9% uptime or customer gets credits |
| **Error Budget** | How much failure is allowed | 0.5% = 3.6 hours downtime/month |

### Choosing SLIs

```
Request-driven service (API, web):
  - Availability: % of requests returning non-5xx
  - Latency: % of requests faster than threshold (p99 < 500ms)
  - Correctness: % of requests returning expected result

Pipeline/batch service:
  - Freshness: % of data updated within SLO window
  - Throughput: records processed per hour
  - Completeness: % of expected records processed
```

### Error Budget Policy

```
Budget remaining > 50%: Ship freely, experiment
Budget remaining 25-50%: Caution — prefer stability over features
Budget remaining < 25%: Freeze deploys, focus on reliability
Budget exhausted (0%): All engineering shifts to reliability
```

## Dashboard Design

### The Four Golden Signals (per service)

```
1. Latency    → p50, p95, p99 response time
2. Traffic    → Requests per second
3. Errors     → Error rate (5xx / total)
4. Saturation → CPU, memory, disk, queue depth
```

### Dashboard Layout

```
Row 1: SLO status (green/yellow/red) + error budget remaining
Row 2: Golden signals (latency, traffic, errors, saturation)
Row 3: Business metrics (orders/min, signups, revenue)
Row 4: Dependencies (DB latency, cache hit rate, external API status)
```

**Rules:**
- Start from user impact (SLO), drill down to infra
- Show rate of change, not just current value (is it getting worse?)
- Use consistent time windows across panels (last 1h, 24h, 7d)
- Every dashboard has an owner (team that maintains it)

## Alerting Rules

### Alert on Symptoms, Not Causes

```
Good (symptom): "Error rate > 5% for 5 minutes"
Bad (cause):    "CPU > 80%"  (might be normal for batch jobs)

Good (symptom): "p99 latency > 2s for 10 minutes"
Bad (cause):    "Database connections > 90"  (might recover on its own)
```

### Severity Levels

| Severity | Criteria | Response | Example |
|----------|----------|----------|---------|
| **P1 Critical** | SLO violated, users impacted NOW | Page on-call immediately | API returning 500 for all users |
| **P2 High** | SLO at risk, degraded experience | Notify team, respond within 1h | Latency 3x normal, error rate rising |
| **P3 Medium** | Anomaly, no user impact yet | Ticket, address within 1 business day | Error budget burning faster than expected |
| **P4 Low** | Informational, trend alert | Review in next sprint | Disk usage growing 5%/week |

### Alert Hygiene

- Every alert has a runbook link (what to do when it fires)
- Review and tune alerts monthly (delete noisy alerts)
- Target < 5 pages per on-call shift per week
- If an alert fires and no action is needed → fix the alert, not the person

## Runbook Template

```markdown
# Alert: API Error Rate > 5%

## Impact
User-facing API returning errors. Orders cannot be placed.

## Quick Check
1. Check dashboard: <link>
2. Check recent deploys: `git log --since="2 hours ago" --oneline`
3. Check dependency status: <status page links>

## Common Causes & Fixes
| Cause | Check | Fix |
|-------|-------|-----|
| Bad deploy | Error started after deploy? | Rollback: `deploy rollback` |
| DB overload | DB dashboard shows high latency? | Scale read replicas |
| Dependency down | External API returning errors? | Enable circuit breaker fallback |

## Escalation
If not resolved in 30 min: page @backend-lead
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We'll add monitoring after launch" | Launch without monitoring = flying blind. Add basic golden signals before first deploy. |
| "Alert on everything, we'll tune later" | Alert fatigue kills on-call morale. Start with SLO-based alerts only. |
| "CPU/memory alerts are enough" | Infrastructure metrics don't tell you if users are happy. Alert on symptoms. |
| "We don't need SLOs yet" | Without SLOs, every incident is a fire drill. Define "healthy" before it breaks. |

## Red Flags

- No monitoring on a production service
- Alerts without runbooks
- Alerting on infrastructure metrics instead of user-facing symptoms
- More than 10 pages per on-call shift per week (alert fatigue)
- Dashboards showing only "up/down" (no latency, error rate, saturation)
- SLOs defined but never reviewed or enforced
- No error budget tracking

## Verification

- [ ] SLIs defined for key user journeys
- [ ] SLOs set with error budget tracking
- [ ] Dashboard shows four golden signals (latency, traffic, errors, saturation)
- [ ] Alerts fire on symptoms (error rate, latency), not causes (CPU, memory)
- [ ] Every alert has a severity level and runbook link
- [ ] On-call rotation configured with escalation policy
- [ ] Alert noise < 5 pages per shift per week
