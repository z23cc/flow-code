# Security Checklist

Quick reference for application security. Stack-agnostic — applies to Rust, Python, Go, TypeScript, and beyond.

## Table of Contents

- [Pre-Commit Checks](#pre-commit-checks)
- [Authentication](#authentication)
- [Authorization](#authorization)
- [Input Validation](#input-validation)
- [Cryptography](#cryptography)
- [Data Protection](#data-protection)
- [Dependency Security](#dependency-security)
- [Error Handling](#error-handling)
- [Infrastructure](#infrastructure)
- [OWASP Top 10 Quick Reference](#owasp-top-10-quick-reference)

## Pre-Commit Checks

- [ ] No secrets in code (passwords, API keys, tokens, private keys)
- [ ] `.gitignore` covers: `.env`, `*.pem`, `*.key`, `credentials.*`
- [ ] `.env.example` uses placeholder values only
- [ ] No hardcoded connection strings or endpoints
- [ ] Secret scanning enabled in CI (e.g., `gitleaks`, `trufflehog`)

## Authentication

- [ ] Passwords hashed with bcrypt (>=12 rounds), scrypt, or argon2
- [ ] Session tokens are cryptographically random (>=256 bits)
- [ ] Session expiration configured with reasonable max-age
- [ ] Rate limiting on login endpoints (<=10 attempts per 15 minutes)
- [ ] Password reset tokens: time-limited (<=1 hour), single-use
- [ ] Account lockout after repeated failures (with notification)
- [ ] MFA supported for sensitive operations

## Authorization

- [ ] Every protected endpoint checks authentication first
- [ ] Every resource access checks ownership or role (prevents IDOR)
- [ ] Admin endpoints require admin role verification
- [ ] API keys/tokens scoped to minimum necessary permissions
- [ ] JWT tokens validated: signature, expiration, issuer, audience
- [ ] Default deny: new endpoints are protected unless explicitly public
- [ ] Service-to-service calls authenticated (mTLS, signed tokens)

## Input Validation

- [ ] All user input validated at system boundaries
- [ ] Validation uses allowlists, not denylists
- [ ] String lengths constrained (min/max)
- [ ] Numeric ranges validated and bounds-checked
- [ ] File uploads: type restricted, size limited, content verified
- [ ] SQL queries parameterized (no string concatenation)
- [ ] Output encoded/escaped for context (HTML, SQL, shell, URLs)
- [ ] URLs validated before redirect (prevent open redirect)
- [ ] Deserialization restricted to expected types (no arbitrary object creation)

## Cryptography

- [ ] TLS 1.2+ for all network communication
- [ ] No custom crypto implementations (use audited libraries)
- [ ] Symmetric encryption: AES-256-GCM or ChaCha20-Poly1305
- [ ] Asymmetric: RSA >=2048 bits or Ed25519
- [ ] Random values from CSPRNG, not `rand()` or `Math.random()`
- [ ] Keys rotated on schedule and on compromise

| Language | Crypto Library | CSPRNG |
|----------|---------------|--------|
| Rust | `ring`, `rustls` | `rand::rngs::OsRng` |
| Python | `cryptography` | `secrets` module |
| Go | `crypto/*` stdlib | `crypto/rand` |
| TypeScript | `crypto` (Node) | `crypto.randomBytes` |

## Data Protection

- [ ] Sensitive fields excluded from API responses (password hashes, tokens)
- [ ] Sensitive data not logged (passwords, tokens, PII)
- [ ] PII encrypted at rest (if required by regulation)
- [ ] Database backups encrypted
- [ ] Data retention policy defined and enforced
- [ ] Personally identifiable data deletable on request

## Dependency Security

- [ ] Dependencies audited regularly for known vulnerabilities
- [ ] Lockfile committed and used in CI (reproducible builds)
- [ ] Automated alerts for vulnerable dependencies enabled
- [ ] Minimal dependency policy (fewer deps = smaller attack surface)

| Language | Audit Command |
|----------|--------------|
| Rust | `cargo audit` |
| Python | `pip-audit`, `safety check` |
| Go | `govulncheck ./...` |
| TypeScript | `npm audit`, `yarn audit` |

## Error Handling

- [ ] Production errors are generic (no stack traces, SQL, or internals)
- [ ] Errors logged server-side with correlation IDs
- [ ] Error messages do not reveal user existence (login/signup)
- [ ] Panic/crash recovery does not expose sensitive state
- [ ] Rate-limited error responses (prevent enumeration attacks)

## Infrastructure

- [ ] HTTPS enforced (redirect HTTP to HTTPS)
- [ ] Security headers set (CSP, HSTS, X-Content-Type-Options, X-Frame-Options)
- [ ] CORS restricted to known origins (never `*` in production)
- [ ] Containers run as non-root user
- [ ] Network segmentation between services and databases
- [ ] Secrets managed via vault/KMS, not environment variables in code

## OWASP Top 10 Quick Reference

| # | Vulnerability | Prevention |
|---|---|---|
| 1 | Broken Access Control | Auth checks on every endpoint, ownership verification |
| 2 | Cryptographic Failures | HTTPS, strong hashing, no secrets in code |
| 3 | Injection | Parameterized queries, input validation |
| 4 | Insecure Design | Threat modeling, spec-driven development |
| 5 | Security Misconfiguration | Security headers, minimal permissions, audit deps |
| 6 | Vulnerable Components | Audit deps, keep updated, minimize dependencies |
| 7 | Auth Failures | Strong passwords, rate limiting, session management |
| 8 | Data Integrity Failures | Verify updates/dependencies, signed artifacts |
| 9 | Logging Failures | Log security events, don't log secrets |
| 10 | SSRF | Validate/allowlist URLs, restrict outbound requests |
