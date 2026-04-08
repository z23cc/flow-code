# Security Checklist

Quick reference for `flow-code-security` and code review. OWASP Top 10 prevention.

## Input Validation

- [ ] All user input validated with schema (Zod, Joi, Pydantic) at system boundary
- [ ] SQL queries use parameterized statements (never string concatenation)
- [ ] OS commands use `execFile` with argument arrays (never `exec` with template strings)
- [ ] File paths validated and sandboxed (no path traversal: `../`)
- [ ] Regular expressions tested for ReDoS (no catastrophic backtracking)

## Authentication

- [ ] Passwords hashed with bcrypt/scrypt/argon2 (never MD5/SHA1/plaintext)
- [ ] Account lockout after 5 failed login attempts
- [ ] Session tokens rotated after login
- [ ] Session expiry configured (idle + absolute timeout)
- [ ] Constant-time comparison for token/password verification
- [ ] Multi-factor authentication available for sensitive operations

## Authorization

- [ ] Every endpoint checks both authentication AND authorization
- [ ] Resource access verifies ownership (not just "is logged in")
- [ ] Principle of least privilege for service accounts
- [ ] Admin operations require elevated permissions (not just role check)

## Data Protection

- [ ] Sensitive data encrypted at rest (AES-256)
- [ ] TLS 1.2+ for all data in transit (no mixed content)
- [ ] PII masked in logs: `email: "j***@example.com"`
- [ ] API responses return only needed fields (DTOs, not raw models)
- [ ] `Cache-Control: no-store` for pages with sensitive data
- [ ] No secrets in source code, environment variables, or logs

## HTTP Security Headers

```
Content-Security-Policy: default-src 'self';
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Strict-Transport-Security: max-age=31536000; includeSubDomains
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: camera=(), microphone=(), geolocation=()
```

## Cookies

- [ ] Auth cookies: `httpOnly`, `secure`, `sameSite=Strict` (or `Lax`)
- [ ] No session data in `localStorage` (vulnerable to XSS)
- [ ] CSRF tokens on state-changing requests

## File Uploads

- [ ] MIME type validated from file content (not just extension)
- [ ] Maximum file size enforced server-side
- [ ] Filenames regenerated (never use user-provided names)
- [ ] Files stored outside web root
- [ ] `Content-Disposition: attachment` on downloads

## Dependencies

- [ ] `npm audit` / `cargo audit` / `pip audit` run in CI
- [ ] Critical/High vulnerabilities in production deps fixed immediately
- [ ] Dependency lockfile committed (`package-lock.json`, `Cargo.lock`)
- [ ] No unused dependencies (attack surface reduction)

## Secrets Management

- [ ] `.env` in `.gitignore` (always)
- [ ] `.env.example` with placeholder values committed
- [ ] Secrets rotated immediately if ever committed to git
- [ ] Production secrets in secrets manager (Vault, AWS SM, GCP SM)
- [ ] No secrets in CI logs, error messages, or API responses

## Rate Limiting

- [ ] Auth endpoints: 5 attempts per minute per IP
- [ ] API endpoints: appropriate limits per user/key
- [ ] Public endpoints: DDoS protection (Cloudflare, WAF)
- [ ] Rate limit headers returned: `X-RateLimit-Remaining`

## Quick Audit

```bash
# Check for secrets in staged files
git diff --cached | grep -iE '(password|secret|token|api_key|private_key)\s*='

# Check for dangerous patterns
grep -rn 'eval(' --include='*.ts' --include='*.js' src/
grep -rn 'dangerouslySetInnerHTML' --include='*.tsx' --include='*.jsx' src/
grep -rn 'innerHTML' --include='*.ts' --include='*.js' src/
```
