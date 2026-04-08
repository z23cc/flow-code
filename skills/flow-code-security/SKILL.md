---
name: flow-code-security
description: "Use when writing code that handles user input, authentication, authorization, secrets, file uploads, or external data. Enforces OWASP Top 10 prevention and three-tier security boundaries."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: security,auth,validation,owasp -->

# Security Hardening

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

## flowctl Setup

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Overview

Security-first development that prevents vulnerabilities at write time rather than catching them in review. Every code change that touches user input, auth, or external data MUST follow these patterns.

## When to Use

- Handling user input (forms, API parameters, URL params, headers)
- Authentication or authorization logic
- Secrets, tokens, API keys, credentials
- File uploads or downloads
- Database queries (especially dynamic ones)
- CORS, CSP, or security header configuration
- Third-party API integration
- Session management

**When NOT to use:**
- Pure UI layout changes with no data handling
- Internal tooling with no external exposure
- Read-only documentation changes

## Three-Tier Boundary System

### Always Do (no exceptions)

- Validate and sanitize ALL user input at system boundaries
- Use parameterized queries — NEVER string concatenation for SQL
- Escape output for the target context (HTML, URL, SQL, shell)
- Use `httpOnly`, `secure`, `sameSite` flags on auth cookies
- Hash passwords with bcrypt/scrypt/argon2 — NEVER MD5/SHA1
- Set security headers: `Content-Security-Policy`, `X-Content-Type-Options`, `X-Frame-Options`
- Use HTTPS everywhere — no mixed content
- Apply principle of least privilege to all service accounts and tokens
- Log security events (login, failed auth, permission changes) — NEVER log credentials

### Ask First (requires explicit justification in task spec)

- Changing auth middleware or session handling
- Adding new user roles or permission levels
- Modifying CORS configuration
- Adding new sensitive data types to the schema
- Exposing new public API endpoints
- Changing rate limiting configuration
- Adding file upload capabilities
- Integrating new third-party services with data access

### Never Do

- Commit secrets, tokens, or credentials (`.env`, API keys, private keys)
- Log passwords, tokens, session IDs, or PII
- Trust client-side validation as the only check
- Disable security headers or CSRF protection
- Use `eval()`, `innerHTML`, or `dangerouslySetInnerHTML` with user data
- Store sessions or tokens in `localStorage` (use `httpOnly` cookies)
- Expose stack traces or internal errors to users
- Use `--no-verify` to skip security hooks

## OWASP Top 10 Prevention

### Injection (SQL, NoSQL, OS Command, LDAP)

```typescript
// ALWAYS: parameterized queries
const user = await db.query('SELECT * FROM users WHERE id = $1', [userId]);

// NEVER: string concatenation
const user = await db.query(`SELECT * FROM users WHERE id = ${userId}`); // VULNERABLE
```

For shell commands:
```typescript
// ALWAYS: use execFile with argument array
execFile('convert', [inputPath, '-resize', '100x100', outputPath]);

// NEVER: template strings in exec
exec(`convert ${inputPath} -resize 100x100 ${outputPath}`); // VULNERABLE
```

### Broken Authentication

- Enforce minimum password length (12+ characters)
- Implement account lockout after 5 failed attempts
- Use constant-time comparison for token validation
- Rotate session tokens after login
- Set session expiry (idle timeout + absolute timeout)

### XSS (Cross-Site Scripting)

```typescript
// ALWAYS: use framework's built-in escaping
<p>{user.name}</p>  // React auto-escapes

// NEVER: bypass escaping
<div dangerouslySetInnerHTML={{__html: user.bio}} />  // VULNERABLE unless sanitized

// If you MUST render HTML, sanitize first:
import DOMPurify from 'dompurify';
<div dangerouslySetInnerHTML={{__html: DOMPurify.sanitize(user.bio)}} />
```

### Broken Access Control

```typescript
// ALWAYS: check ownership, not just authentication
async function getDocument(userId: string, docId: string) {
  const doc = await db.documents.findById(docId);
  if (doc.ownerId !== userId) throw new ForbiddenError();
  return doc;
}

// NEVER: assume authenticated = authorized
async function getDocument(docId: string) {
  return db.documents.findById(docId); // Missing ownership check
}
```

### Security Misconfiguration

- Disable debug mode in production
- Remove default credentials and sample data
- Keep dependencies updated — run `npm audit` / `cargo audit` regularly
- Disable directory listing on web servers
- Set `X-Content-Type-Options: nosniff`

### Sensitive Data Exposure

- Encrypt sensitive data at rest (AES-256)
- Use TLS 1.2+ for data in transit
- Never return more data than needed (use DTOs, not raw models)
- Mask sensitive fields in logs: `email: "j***@example.com"`
- Set `Cache-Control: no-store` for pages with sensitive data

## Input Validation Patterns

Validate at the boundary, trust internally:

```typescript
// Schema validation at API boundary
import { z } from 'zod';

const CreateUserSchema = z.object({
  email: z.string().email().max(255),
  name: z.string().min(1).max(100).trim(),
  age: z.number().int().min(13).max(150).optional(),
});

// Validate once at the edge
function createUser(req: Request) {
  const input = CreateUserSchema.parse(req.body); // throws on invalid
  return userService.create(input); // internal code trusts validated input
}
```

### File Upload Safety

- Validate MIME type from file content (not just extension)
- Enforce maximum file size server-side
- Generate new filenames (never use user-provided names)
- Store uploads outside the web root
- Scan for malware if accepting documents
- Set `Content-Disposition: attachment` for downloads

## Secrets Management

```bash
# Check for secrets before committing
git diff --cached --name-only | xargs grep -l -E '(password|secret|token|api_key|private_key)=' || true

# Use environment variables, not hardcoded values
DATABASE_URL=env("DATABASE_URL")  # Good
DATABASE_URL="postgres://user:pass@host/db"  # NEVER
```

- Use `.env.example` with placeholder values (never real secrets)
- Add `.env` to `.gitignore` ALWAYS
- Rotate secrets immediately if committed (even to a branch)
- Use a secrets manager (Vault, AWS Secrets Manager) in production

## npm/cargo Audit Triage

When `npm audit` or `cargo audit` reports vulnerabilities:

```
Critical/High + in production deps + exploitable → Fix immediately
Critical/High + in dev deps only → Fix in next sprint
Medium + has patch available → Update dependency
Low + no patch → Document and track, don't block
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "This is an internal tool" | Internal tools get compromised. Same standards apply. |
| "We'll add auth later" | Unauthenticated endpoints get discovered. Add auth from day one. |
| "Client-side validation is enough" | Any HTTP client bypasses it. Server validation is the real check. |
| "This secret is temporary" | Temporary secrets get committed and stay in git history forever. |
| "Nobody would guess this endpoint" | Security through obscurity is not security. |
| "We trust our users" | Assume all input is adversarial. Always validate. |

## Red Flags

- SQL queries built with string concatenation or template literals
- `innerHTML` or `dangerouslySetInnerHTML` with user-provided content
- Passwords stored as plaintext or MD5/SHA1
- API keys or secrets in source code
- Missing rate limiting on auth endpoints
- `CORS: *` (allow all origins) in production
- `eval()` with any external input
- Error messages exposing stack traces or internal paths
- Session tokens in URL parameters or localStorage
- Missing CSRF protection on state-changing endpoints

## Verification

After writing security-sensitive code:

- [ ] No secrets in source code (grep for password, secret, token, api_key)
- [ ] All user input validated at the boundary with schema validation
- [ ] SQL/NoSQL queries use parameterized queries (no string concatenation)
- [ ] Output escaped for target context (HTML, URL, shell)
- [ ] Auth checks verify both authentication AND authorization (ownership)
- [ ] Security headers configured (CSP, X-Content-Type-Options, X-Frame-Options)
- [ ] Sensitive data not logged or exposed in error messages
- [ ] File uploads validated (MIME type, size, renamed, stored safely)
- [ ] Dependencies audited (`npm audit` / `cargo audit` clean or triaged)
- [ ] Rate limiting configured on auth and public endpoints
