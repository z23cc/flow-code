---
name: flow-code-realtime
description: "Use when implementing WebSocket, SSE, long-polling, or any real-time communication. Covers protocol selection, connection management, reconnection, and scaling."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: websocket,sse,realtime,streaming -->

# Real-Time Communication

## Overview

Choose the right real-time protocol for the use case, handle connection lifecycle properly, and plan for reconnection from day one. Real-time is not "just add WebSocket" — it's connection management, state synchronization, and graceful degradation.

## Protocol Selection

| Protocol | Direction | Use When | Don't Use When |
|----------|----------|----------|---------------|
| **WebSocket** | Bidirectional | Chat, gaming, collaborative editing | Simple notifications, read-only updates |
| **SSE (Server-Sent Events)** | Server→Client | Live feeds, notifications, progress | Client needs to send frequent data |
| **Long Polling** | Simulated bidirectional | WebSocket/SSE blocked by proxy | High-frequency updates (> 1/sec) |
| **HTTP Polling** | Client-initiated | Infrequent updates (> 30s interval) | Real-time requirements (< 5s latency) |

**Default choice: SSE** for most real-time needs. Simpler than WebSocket, auto-reconnects, works through proxies, HTTP/2 multiplexed.

## WebSocket Pattern

```typescript
// Server
const wss = new WebSocketServer({ server });
wss.on('connection', (ws, req) => {
  const userId = authenticateFromHeaders(req);
  if (!userId) { ws.close(4001, 'Unauthorized'); return; }

  ws.on('message', (data) => {
    const msg = JSON.parse(data.toString());
    handleMessage(userId, msg);
  });

  ws.on('close', () => cleanup(userId));
  ws.on('error', (err) => logger.error('ws.error', { userId, err }));

  // Heartbeat
  ws.isAlive = true;
  ws.on('pong', () => { ws.isAlive = true; });
});

// Heartbeat interval (detect dead connections)
setInterval(() => {
  wss.clients.forEach(ws => {
    if (!ws.isAlive) { ws.terminate(); return; }
    ws.isAlive = false;
    ws.ping();
  });
}, 30_000);
```

```typescript
// Client with reconnection
class ReconnectingWebSocket {
  private ws: WebSocket | null = null;
  private retryCount = 0;
  private maxRetries = 10;

  connect(url: string) {
    this.ws = new WebSocket(url);
    this.ws.onopen = () => { this.retryCount = 0; };
    this.ws.onclose = (e) => {
      if (e.code !== 1000) this.reconnect(url); // Not intentional close
    };
    this.ws.onerror = () => {}; // onclose will fire after this
  }

  private reconnect(url: string) {
    if (this.retryCount >= this.maxRetries) return;
    const delay = Math.min(1000 * 2 ** this.retryCount, 30000);
    setTimeout(() => { this.retryCount++; this.connect(url); }, delay);
  }
}
```

## SSE Pattern

```typescript
// Server (Express)
app.get('/events', authenticate, (req, res) => {
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');

  const send = (event: string, data: unknown) => {
    res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  };

  send('connected', { userId: req.userId });
  const unsub = eventBus.subscribe(req.userId, send);
  req.on('close', unsub);
});
```

```typescript
// Client (auto-reconnects by default!)
const events = new EventSource('/events', { withCredentials: true });
events.addEventListener('order.updated', (e) => {
  const order = JSON.parse(e.data);
  updateOrderInUI(order);
});
events.onerror = () => { /* browser auto-reconnects */ };
```

## Connection Lifecycle Rules

- Always authenticate on connection (not after)
- Implement heartbeat/ping-pong (detect dead connections)
- Client must auto-reconnect with exponential backoff
- Server must clean up resources on disconnect
- Use connection IDs for debugging (log with every message)
- Set maximum connection duration (prevent resource leaks)
- Handle message ordering (sequence numbers for critical data)

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "WebSocket for everything" | SSE is simpler and sufficient for most server→client pushes. |
| "Reconnection can wait" | Users lose connection constantly (mobile, WiFi switches). Handle it from day one. |
| "We don't need heartbeats" | Without heartbeats, dead connections hold resources for hours. |
| "HTTP polling is outdated" | For updates every 30+ seconds, polling is simpler and more reliable than persistent connections. |

## Red Flags

- No reconnection logic on client
- No heartbeat/ping on server
- Authentication after connection (race condition)
- No connection cleanup on server disconnect
- Broadcasting to all connections instead of targeted delivery
- Missing error handling on message parse
- No rate limiting on inbound WebSocket messages

## Verification

- [ ] Protocol matches use case (SSE for server→client, WebSocket for bidirectional)
- [ ] Authentication on connection establishment
- [ ] Client auto-reconnects with exponential backoff
- [ ] Server heartbeat detects dead connections
- [ ] Resources cleaned up on disconnect
- [ ] Error handling for malformed messages
- [ ] Graceful degradation if real-time unavailable
