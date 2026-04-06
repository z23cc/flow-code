# Security Checklist

Quick reference for security in flow-code. CLI tool handling local state, no web server.

## Pre-Commit Secret Scan

```bash
# Check staged files for secrets before committing
git diff --cached | grep -iE \
  "password\s*=|secret\s*=|api_key\s*=|token\s*=|private_key|BEGIN RSA|BEGIN EC"

# Patterns to catch
grep -rn "AKIA[0-9A-Z]{16}"              # AWS access keys
grep -rn "sk-[a-zA-Z0-9]{48}"            # OpenAI API keys
grep -rn "ghp_[a-zA-Z0-9]{36}"           # GitHub personal tokens
grep -rn "xoxb-[0-9]{10,}"               # Slack bot tokens
```

## Files to Never Commit

| File/Pattern | Reason | Mitigation |
|---|---|---|
| `.env` | Contains secrets | Add to `.gitignore` |
| `*.pem`, `*.key` | Private keys | Add to `.gitignore` |
| `.flow/` | Runtime state (may contain tokens) | Add to `.gitignore` |
| `credentials.json` | Service account keys | Add to `.gitignore` |
| `*.upstream` | Backup files with potential secrets | Add to `.gitignore` |

Commit `.env.example` with placeholder values instead of `.env`.

## Secrets Management

| Do | Don't |
|---|---|
| Use environment variables for secrets | Hardcode secrets in source |
| Commit `.env.example` with placeholders | Commit `.env` with real values |
| Use `--json` flag for machine parsing | Parse human-readable output with secrets |
| Rotate secrets after exposure | Ignore leaked credentials |
| Scope API keys to minimum permissions | Use admin keys for all operations |

## OWASP Top 10 Quick Reference

| # | Vulnerability | Prevention |
|---|---|---|
| 1 | Broken Access Control | Auth checks on every endpoint, ownership verification |
| 2 | Cryptographic Failures | HTTPS, strong hashing, no secrets in code |
| 3 | Injection | Parameterized queries, input validation |
| 4 | Insecure Design | Threat modeling, spec-driven development |
| 5 | Security Misconfiguration | Minimal permissions, audit deps, secure defaults |
| 6 | Vulnerable Components | `cargo audit`, `cargo deny`, minimal deps |
| 7 | Auth Failures | Strong passwords, rate limiting, session management |
| 8 | Data Integrity Failures | Verify updates/dependencies, signed artifacts |
| 9 | Logging Failures | Log security events, don't log secrets |
| 10 | SSRF | Validate/allowlist URLs, restrict outbound requests |

## Dependency Audit (Rust)

```bash
# Audit for known vulnerabilities
cargo audit

# Deny specific licenses or advisories
cargo deny check

# Check for outdated dependencies
cargo outdated

# Minimal dependency tree (fewer deps = smaller attack surface)
cargo tree | wc -l
```

| Tool | Purpose | When to Run |
|---|---|---|
| `cargo audit` | CVE database check | Pre-commit, CI |
| `cargo deny` | License + advisory policy | CI pipeline |
| `cargo outdated` | Stale dependency check | Weekly/monthly |
| `cargo tree` | Dependency graph audit | When adding deps |

## Input Validation

| Rule | Example |
|---|---|
| Validate at boundaries only | CLI args, JSON input, file reads |
| Use allowlists, not denylists | Accept known-good, reject everything else |
| Constrain string lengths | Task titles: 1-500 chars |
| Validate file paths | No `..` traversal, stay within `.flow/` |
| Sanitize output for shells | Escape special chars in generated commands |
| Parse, don't validate | Deserialize into typed structs, not raw strings |

```rust
// Good: parse into typed struct at boundary
let task: TaskInput = serde_json::from_str(&input)?;

// Bad: pass raw string around and validate later
fn process(raw: &str) { /* who validates this? */ }
```

## Error Handling

| Context | Approach |
|---|---|
| CLI output (user-facing) | Clear error message, no stack traces |
| JSON output (`--json`) | Structured error: `{"error": "message"}` |
| Internal logging | Full context, file/line, but no secrets |
| Panics | Catch at boundaries with `catch_unwind` or `?` |

```rust
// Good: generic error to user, detail in logs
eprintln!("Error: failed to open database");
tracing::error!("DB open failed: {:?} at {:?}", err, path);

// Bad: expose internals
eprintln!("Error: {}", err);  // May leak path, SQL, etc.
```

## SQL Safety (libSQL)

| Do | Don't |
|---|---|
| Use parameterized queries | Concatenate user input into SQL |
| Use `?` placeholders | Use `format!()` for query building |
| Validate types before query | Trust raw input as safe |
| Use transactions for multi-step ops | Execute multiple queries without atomicity |

```rust
// Good: parameterized
conn.execute("INSERT INTO tasks (title) VALUES (?1)", [&title]).await?;

// Bad: string concatenation
conn.execute(&format!("INSERT INTO tasks (title) VALUES ('{}')", title), ()).await?;
```

## File System Security

| Rule | Rationale |
|---|---|
| Validate paths stay within `.flow/` | Prevent directory traversal |
| Use temp dirs for scratch work | Don't pollute user directories |
| Set restrictive permissions on state files | Protect `.flow/` data |
| Clean up temp files on exit | No stale sensitive data |
| Don't follow symlinks blindly | Potential symlink attacks |
