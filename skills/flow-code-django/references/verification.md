# Django Pre-Deployment Verification

12-phase verification loop. Run before PRs, after major changes, and pre-deploy.

## Phase 1: Environment

```bash
python --version
which python
pip list --outdated
```

## Phase 2: Code Quality

```bash
uv run mypy . --config-file pyproject.toml
uv run ruff check . --fix
uv run ruff format --check .
uv run python manage.py check --deploy
```

## Phase 3: Migrations

```bash
python manage.py showmigrations
python manage.py makemigrations --check
python manage.py migrate --plan
python manage.py migrate
```

## Phase 4: Tests + Coverage

```bash
pytest --cov=apps --cov-report=html --cov-report=term-missing --reuse-db
pytest -m "not slow"        # Skip slow tests
pytest apps/users/tests/    # Specific app
```

## Phase 5: Security Scan

```bash
pip-audit
safety check --full-report
python manage.py check --deploy
bandit -r . -f json -o bandit-report.json
gitleaks detect --source . --verbose
```

## Phase 6: Django Commands

```bash
python manage.py check
python manage.py collectstatic --noinput --clear
python manage.py check --database default
```

## Phase 7: Performance

Check for N+1 queries, missing indexes, duplicate queries.

## Phase 8: Static Assets

```bash
npm audit
npm run build
ls -la staticfiles/
```

## Phase 9: Configuration Review

```python
# Verify in Django shell
checks = {
    'DEBUG is False': not settings.DEBUG,
    'SECRET_KEY set': bool(settings.SECRET_KEY and len(settings.SECRET_KEY) > 30),
    'ALLOWED_HOSTS set': len(settings.ALLOWED_HOSTS) > 0,
    'HTTPS enabled': getattr(settings, 'SECURE_SSL_REDIRECT', False),
    'HSTS enabled': getattr(settings, 'SECURE_HSTS_SECONDS', 0) > 0,
    'Not SQLite': settings.DATABASES['default']['ENGINE'] != 'django.db.backends.sqlite3',
}
```

## Phase 10: Logging

```bash
python manage.py shell -c "import logging; logging.getLogger('django').warning('Test')"
```

## Phase 11: API Documentation

```bash
python manage.py generateschema --format openapi-json > schema.json
python -c "import json; json.load(open('schema.json'))"
```

## Phase 12: Diff Review

```bash
git diff --stat
git diff | grep -i "todo\|fixme\|hack"
git diff | grep "print("
git diff | grep "DEBUG = True"
git diff | grep "import pdb"
```

## Pre-Deployment Checklist

- [ ] All tests passing
- [ ] Coverage >= 80%
- [ ] No security vulnerabilities
- [ ] No unapplied migrations
- [ ] DEBUG = False
- [ ] SECRET_KEY from environment
- [ ] ALLOWED_HOSTS set
- [ ] Database backups enabled
- [ ] Static files collected
- [ ] Logging configured
- [ ] Error monitoring (Sentry) configured
- [ ] HTTPS/SSL configured
- [ ] Environment variables documented

## GitHub Actions

```yaml
name: Django Verification
on: [push, pull_request]

jobs:
  verify:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5

    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-python@v4
        with:
          python-version: '3.11'

      - name: Install
        run: |
          uv sync
          uv pip install ruff mypy pytest pytest-django pytest-cov bandit

      - name: Code quality
        run: |
          ruff check .
          black . --check
          mypy .

      - name: Security
        run: |
          bandit -r . -f json -o bandit-report.json
          pip-audit

      - name: Tests
        env:
          DATABASE_URL: postgres://postgres:postgres@localhost:5432/test
          DJANGO_SECRET_KEY: test-secret-key
        run: pytest --cov=apps --cov-report=xml

      - uses: codecov/codecov-action@v3
```

## Quick Reference

| Check | Command |
|-------|---------|
| Type check | `mypy .` |
| Lint | `ruff check .` |
| Format | `ruff format --check .` |
| Migrations | `python manage.py makemigrations --check` |
| Tests | `pytest --cov=apps` |
| Security | `pip-audit && bandit -r .` |
| Django check | `python manage.py check --deploy` |
| Static | `python manage.py collectstatic --noinput` |
