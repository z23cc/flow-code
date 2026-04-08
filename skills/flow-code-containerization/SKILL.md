---
name: flow-code-containerization
description: "Use when writing Dockerfiles, docker-compose configs, or Kubernetes manifests. Covers image optimization, multi-stage builds, health checks, and security hardening."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: docker,kubernetes,containers,deployment -->

# Containerization

## Overview

Build small, secure, reproducible container images. Containers should be immutable, stateless, and fast to start. Treat Dockerfiles as production code — review them with the same rigor.

## When to Use

- Writing or modifying Dockerfiles
- Setting up docker-compose for local development
- Creating Kubernetes manifests (Deployment, Service, Ingress)
- Optimizing image size or build time
- Adding health checks to containerized services

## Dockerfile Best Practices

### Multi-Stage Build

```dockerfile
# Stage 1: Build
FROM node:20-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci --production=false
COPY . .
RUN npm run build

# Stage 2: Production (minimal image)
FROM node:20-alpine
WORKDIR /app
RUN addgroup -g 1001 appgroup && adduser -u 1001 -G appgroup -s /bin/sh -D appuser
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./
USER appuser
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s CMD wget -qO- http://localhost:3000/healthz || exit 1
CMD ["node", "dist/index.js"]
```

### Layer Optimization

```dockerfile
# Good: dependencies cached separately from code
COPY package*.json ./
RUN npm ci
COPY . .

# Bad: cache busted on every code change
COPY . .
RUN npm ci
```

**Rules:**
- Copy dependency files first, install, then copy source code
- Use `.dockerignore` (exclude `node_modules`, `.git`, `dist`, `.env`)
- Pin base image versions (`node:20.11-alpine`, not `node:latest`)
- Combine RUN commands to reduce layers (`RUN apt-get update && apt-get install -y ...`)
- Clean up in the same layer (`&& rm -rf /var/lib/apt/lists/*`)

### Security

- Run as non-root user (`USER appuser`)
- Use `alpine` or `distroless` base images (smaller attack surface)
- Don't copy `.env`, secrets, or SSH keys into images
- Scan images: `docker scout cves` or `trivy image`
- Set read-only filesystem where possible (`--read-only`)

## Docker Compose (Local Dev)

```yaml
services:
  app:
    build: .
    ports: ["3000:3000"]
    environment:
      DATABASE_URL: postgres://user:pass@db:5432/app
    depends_on:
      db: { condition: service_healthy }
    volumes:
      - .:/app        # Hot reload in dev
      - /app/node_modules  # Don't overwrite container's node_modules

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: user
      POSTGRES_PASSWORD: pass
      POSTGRES_DB: app
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: pg_isready -U user -d app
      interval: 5s
      timeout: 3s
      retries: 5

volumes:
  pgdata:
```

**Rules:**
- Use `depends_on` with `condition: service_healthy` (not just service order)
- Mount source code for hot reload in dev, but NOT in production
- Use named volumes for persistent data (not bind mounts)
- Set resource limits in production (`deploy.resources.limits`)

## Kubernetes Essentials

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
spec:
  replicas: 3
  selector:
    matchLabels: { app: app }
  template:
    spec:
      containers:
        - name: app
          image: registry.example.com/app:v1.2.3  # Pin tag, never :latest
          ports: [{ containerPort: 3000 }]
          resources:
            requests: { cpu: "100m", memory: "128Mi" }
            limits: { cpu: "500m", memory: "512Mi" }
          livenessProbe:
            httpGet: { path: /healthz, port: 3000 }
            initialDelaySeconds: 10
            periodSeconds: 15
          readinessProbe:
            httpGet: { path: /readyz, port: 3000 }
            initialDelaySeconds: 5
            periodSeconds: 5
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef: { name: app-secrets, key: database-url }
```

**Rules:**
- Always set resource `requests` AND `limits`
- Use `livenessProbe` (restart if dead) + `readinessProbe` (route traffic when ready)
- Secrets via `secretKeyRef`, not environment variables in manifests
- Pin image tags (not `:latest` in production)
- Use `PodDisruptionBudget` for high-availability services

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "It works on my machine" | That's why containers exist. If it doesn't build in Docker, it doesn't work. |
| "We don't need multi-stage builds" | Your 2GB image with build tools is a security risk and slow to deploy. |
| ":latest is fine for now" | :latest is non-deterministic. You can't rollback to :latest. Pin versions. |
| "Root is easier" | Root in containers = root on host (if container escapes). Always run as non-root. |
| "Health checks are optional" | Without them, K8s sends traffic to broken pods and never restarts hung processes. |

## Red Flags

- `FROM node:latest` (unpinned base image)
- Running as root (no `USER` directive)
- `.env` or secrets COPY'd into image
- No `.dockerignore` (image contains .git, node_modules)
- No health check (`HEALTHCHECK` or K8s probes)
- `COPY . .` before dependency install (cache invalidation)
- No resource limits in K8s manifests
- `:latest` tag in production deployments

## Verification

- [ ] Multi-stage build (build deps not in production image)
- [ ] Non-root user in production
- [ ] Base image pinned to specific version
- [ ] `.dockerignore` excludes `.git`, `node_modules`, `.env`
- [ ] Health check configured (Docker HEALTHCHECK or K8s probes)
- [ ] Dependency install before source copy (layer caching)
- [ ] No secrets in image (use env vars or secret mounts)
- [ ] Image scanned for vulnerabilities
