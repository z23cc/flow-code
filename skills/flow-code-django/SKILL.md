---
name: flow-code-django
description: "Use when working on Django projects — architecture, DRF APIs, ORM patterns, security, testing, and deployment verification. Triggers on Django-related tasks."
tier: 3
user-invocable: false
---

# Django Development

Production-grade Django patterns for architecture, DRF, security, testing, and deployment.

## When to Use

- Building or refactoring Django applications
- Designing REST APIs with Django REST Framework
- Reviewing Django code for security or performance
- Setting up pytest/TDD infrastructure for Django
- Pre-deployment verification of Django projects
- Fixing Django ORM N+1 queries or performance issues

## Quick Navigation

| Need | Reference |
|------|-----------|
| Project structure, settings, service layer | [architecture.md](references/architecture.md) |
| Models, QuerySets, managers, N+1 prevention | [orm-patterns.md](references/orm-patterns.md) |
| Serializers, ViewSets, filtering, pagination | [drf-patterns.md](references/drf-patterns.md) |
| Auth, CSRF, XSS, SQL injection, RBAC | [security.md](references/security.md) |
| pytest setup, factories, mocking, coverage | [testing.md](references/testing.md) |
| 12-phase pre-deploy verification, CI/CD | [verification.md](references/verification.md) |

## 10 Core Patterns

### 1. Split Settings (base/dev/prod/test)
Separate `config/settings/` with base, development, production, test modules. See [architecture.md](references/architecture.md).

### 2. Service Layer
Business logic in `services.py` with `@transaction.atomic()`, not in views or serializers. See [architecture.md](references/architecture.md).

### 3. Custom QuerySet
Chainable query methods (`active()`, `with_category()`, `in_stock()`) via `QuerySet.as_manager()`. See [orm-patterns.md](references/orm-patterns.md).

### 4. N+1 Prevention
`select_related()` for ForeignKey, `prefetch_related()` for ManyToMany. Always in ViewSet `queryset`. See [orm-patterns.md](references/orm-patterns.md).

### 5. Serializer per Action
`get_serializer_class()` returns different serializers for create/list/detail. See [drf-patterns.md](references/drf-patterns.md).

### 6. Permission Classes
`IsOwnerOrReadOnly`, `IsAdminOrReadOnly`, `IsVerifiedUser` — compose on ViewSets. See [security.md](references/security.md).

### 7. Factory Boy + pytest
`DjangoModelFactory` with `SubFactory`, `Sequence`, `PostGenerationMethodCall`. See [testing.md](references/testing.md).

### 8. conftest.py Fixtures
`user`, `admin_user`, `authenticated_client`, `api_client`, `authenticated_api_client`. See [testing.md](references/testing.md).

### 9. Security Settings
`SECURE_SSL_REDIRECT`, `SESSION_COOKIE_SECURE`, `CSRF_COOKIE_SECURE`, HSTS, CSP headers. See [security.md](references/security.md).

### 10. Pre-Deploy Verification
12-phase loop: env -> lint -> migrations -> tests -> security -> performance -> config. See [verification.md](references/verification.md).

## Decision Trees

### CBV vs FBV vs ViewSet
- CRUD on a model -> **ModelViewSet**
- Custom API endpoint -> **@api_view** (FBV)
- Template-based page -> **generic CBV** (ListView, DetailView)
- Complex multi-step form -> **FBV**

### Caching Strategy
- Entire page rarely changes -> **@cache_page**
- Expensive template fragment -> **{% cache %}**
- Computed value reused across requests -> **cache.get/set**
- QuerySet result -> **cache with invalidation on save signal**

### Authentication
- Session-based web app -> **SessionAuthentication**
- Mobile/SPA API -> **JWT (simplejwt)**
- Third-party integrations -> **TokenAuthentication**
- OAuth2 -> **django-allauth + dj-rest-auth**

## Common Mistakes

- Putting business logic in views/serializers instead of `services.py`
- Forgetting `select_related`/`prefetch_related` — N+1 queries in production
- Using `|safe` on user input in templates — XSS vulnerability
- String interpolation in raw SQL — SQL injection
- Skipping test settings optimization (MD5 hasher, `:memory:` SQLite, `DisableMigrations`)
- Not running `manage.py check --deploy` before shipping
- Hardcoding `SECRET_KEY` instead of reading from environment
- Using `DEBUG = True` in production settings
- Creating migrations on test database then applying to prod — always `makemigrations --check`
- Over-mocking: mocking Django internals instead of only external services

## Type Checking

When fixing mypy errors in Django projects:

1. Prefer `cast()` over `type: ignore`
2. Prefer `type: ignore` over runtime assertions
3. For lazy translation strings, guard with `TYPE_CHECKING`:
   ```python
   from typing import TYPE_CHECKING
   if TYPE_CHECKING:
       from django.utils.functional import _StrPromise
   ```
4. Group errors by type, propose fixes, get approval before applying
