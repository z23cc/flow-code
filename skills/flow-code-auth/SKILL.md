---
name: flow-code-auth
description: "Use when implementing authentication, authorization, OAuth, JWT, RBAC, or session management. Covers token lifecycle, permission models, and secure session handling."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: auth,authentication,authorization,jwt,oauth,rbac -->

# Authentication & Authorization

## Overview

Authentication (who are you?) and authorization (what can you do?) are separate concerns. Implement them in separate layers, test them independently, and never trust the client.

## When to Use

- Adding login/signup flows
- Implementing OAuth (Google, GitHub, etc.)
- Designing role-based access control (RBAC)
- Managing JWT tokens or sessions
- Adding API key authentication
- Implementing multi-tenant access control

## Authentication Patterns

### Session-Based (Server-Side)

```
Client                    Server
  │── POST /login ──────>│ Verify credentials
  │<── Set-Cookie: sid ──│ Create session in store
  │── GET /api (cookie) ─>│ Lookup session by cookie
  │<── 200 data ─────────│
```

**Best for:** Traditional web apps, SSR frameworks (Next.js, Rails, Django).

### Token-Based (JWT)

```
Client                    Server
  │── POST /login ──────>│ Verify credentials
  │<── { accessToken, ──│ Sign JWT
  │     refreshToken }   │
  │── GET /api ──────────>│ Verify JWT signature
  │   Authorization:      │ (no DB lookup needed)
  │   Bearer <token>      │
  │<── 200 data ─────────│
```

**Best for:** APIs, SPAs, mobile apps, microservices.

### Token Lifecycle

```
Access Token:  short-lived (15 min), stored in memory
Refresh Token: long-lived (7-30 days), stored in httpOnly cookie
API Key:       no expiry, stored server-side, revocable

Flow:
1. Login → get access + refresh tokens
2. Access token expires → use refresh token to get new pair
3. Refresh token expires → force re-login
4. Logout → revoke refresh token server-side
```

**Rules:**
- Access tokens in memory (not localStorage — XSS vulnerable)
- Refresh tokens in httpOnly secure cookies (not accessible to JS)
- Rotate refresh tokens on every use (detect theft)
- Maintain a revocation list for compromised tokens

### OAuth 2.0 / OIDC

```
1. Redirect user to provider: /authorize?client_id=...&redirect_uri=...&scope=openid
2. User authenticates with provider
3. Provider redirects back with authorization code
4. Server exchanges code for tokens (server-to-server, code never exposed to client)
5. Server creates session or issues own JWT
```

**Rules:**
- Always use Authorization Code flow (not Implicit — deprecated)
- Use PKCE for public clients (SPAs, mobile)
- Validate `id_token` signature and claims (issuer, audience, expiry)
- Store provider tokens server-side (never send to client)
- Request minimum scopes needed

## Authorization Patterns

### RBAC (Role-Based Access Control)

```typescript
// Define roles and permissions
const ROLES = {
  admin:  ['read', 'write', 'delete', 'manage_users'],
  editor: ['read', 'write'],
  viewer: ['read'],
} as const;

// Check permission (not role!)
function requirePermission(permission: string) {
  return (req, res, next) => {
    const userPerms = ROLES[req.user.role];
    if (!userPerms?.includes(permission)) {
      return res.status(403).json({ code: 'FORBIDDEN', message: 'Insufficient permissions' });
    }
    next();
  };
}

// Usage: check permission, not role
app.delete('/api/posts/:id', requirePermission('delete'), deletePost);
```

**Rule:** Check permissions, not roles. Roles map to permissions, but code should check `canDelete`, not `isAdmin`.

### Resource-Level Authorization

```typescript
// ALWAYS check ownership — not just authentication
async function updatePost(userId: string, postId: string, data: Partial<Post>) {
  const post = await db.posts.findById(postId);
  if (!post) throw new NotFoundError();
  if (post.authorId !== userId && !hasPermission(userId, 'admin')) {
    throw new ForbiddenError();  // Authenticated but not authorized
  }
  return db.posts.update(postId, data);
}
```

### Multi-Tenant Isolation

```typescript
// Every query scoped to tenant
function getTenantPosts(tenantId: string, userId: string) {
  return db.posts.findMany({
    where: { tenantId, authorId: userId },  // Always include tenantId
  });
}

// Middleware enforces tenant context
function tenantMiddleware(req, res, next) {
  req.tenantId = extractTenantFromToken(req);
  if (!req.tenantId) return res.status(403).json({ code: 'NO_TENANT' });
  next();
}
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "We'll add auth later" | Every unauthenticated endpoint becomes tech debt. Add auth from day one. |
| "Just check if user is admin" | Check permissions, not roles. Roles change; permissions are the contract. |
| "JWT in localStorage is fine" | Any XSS vulnerability = stolen tokens. Use httpOnly cookies for refresh tokens. |
| "OAuth is too complex for now" | OAuth libraries handle the complexity. Don't build your own auth system. |
| "We trust our API clients" | Never trust clients. Validate permissions server-side for every request. |

## Red Flags

- Passwords stored as plaintext or MD5/SHA1
- JWT stored in localStorage (XSS-vulnerable)
- Authorization checks only in the UI (not server-side)
- Role checks instead of permission checks (`if (user.role === 'admin')`)
- No refresh token rotation (theft undetectable)
- OAuth Implicit flow (deprecated, use Authorization Code + PKCE)
- Missing resource-level auth (authenticated users can access any resource)
- Hardcoded API keys in source code
- No session expiry or token expiry

## Verification

- [ ] Authentication and authorization in separate middleware layers
- [ ] Passwords hashed with bcrypt/scrypt/argon2
- [ ] Access tokens short-lived (15 min), refresh tokens in httpOnly cookies
- [ ] OAuth uses Authorization Code + PKCE (not Implicit)
- [ ] Authorization checks permissions, not roles
- [ ] Resource-level auth verifies ownership (not just authentication)
- [ ] Multi-tenant queries always scoped by tenant ID
- [ ] Token revocation mechanism exists (logout invalidates server-side)
- [ ] No secrets or tokens in localStorage

**See also:** [Security Checklist](../../references/security-checklist.md) for broader security patterns.
