import type { FlowEventType, FlowEventData, TimestampedEvent } from "./types";

export type ConnectionState = "connected" | "disconnected" | "reconnecting";

type ConnectionChangeHandler = (state: ConnectionState) => void;
type EventTypeHandler<T = FlowEventData> = (data: T, timestamp: string) => void;

const MAX_BACKOFF_MS = 30_000;
const PERSISTENT_WARNING_THRESHOLD = 5;

export interface EventConnection {
  /** Current connection state */
  state: ConnectionState;
  /** Register a handler for a specific event type */
  on<T extends FlowEventData>(type: FlowEventType, handler: EventTypeHandler<T>): void;
  /** Register a handler for connection state changes */
  onConnectionChange(handler: ConnectionChangeHandler): void;
  /** Close the connection (no reconnect) */
  close(): void;
}

export function connectEvents(
  onMessage?: (data: unknown) => void,
): EventConnection {
  let ws: WebSocket | null = null;
  let closed = false;
  let failures = 0;
  let currentState: ConnectionState = "disconnected";

  const connectionChangeHandlers: ConnectionChangeHandler[] = [];
  const eventHandlers = new Map<FlowEventType, EventTypeHandler[]>();

  function setState(s: ConnectionState) {
    currentState = s;
    connection.state = s;
    for (const h of connectionChangeHandlers) {
      h(s);
    }
  }

  function backoffMs(): number {
    // 1s, 2s, 4s, 8s, 16s, capped at 30s
    const ms = Math.min(1000 * Math.pow(2, failures), MAX_BACKOFF_MS);
    return ms;
  }

  function connect() {
    if (closed) return;

    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    ws = new WebSocket(`${proto}//${window.location.host}/api/v1/events`);

    ws.onopen = () => {
      failures = 0;
      setState("connected");
    };

    ws.onmessage = (event) => {
      try {
        const raw = JSON.parse(event.data);

        // Legacy callback
        if (onMessage) {
          onMessage(raw);
        }

        // Typed dispatch: TimestampedEvent = {timestamp, event: {type, data?}}
        const stamped = raw as TimestampedEvent;
        if (stamped?.event?.type) {
          const handlers = eventHandlers.get(stamped.event.type);
          if (handlers) {
            for (const h of handlers) {
              h(stamped.event.data ?? null, stamped.timestamp);
            }
          }
        }
      } catch {
        console.warn("Failed to parse WS message:", event.data);
      }
    };

    ws.onerror = () => {
      // onerror is always followed by onclose, handle reconnect there
    };

    ws.onclose = () => {
      if (closed) {
        setState("disconnected");
        return;
      }

      failures++;
      setState("reconnecting");

      if (failures >= PERSISTENT_WARNING_THRESHOLD) {
        console.warn(
          `WebSocket reconnect attempt ${failures} — daemon may be restarting`,
        );
      }

      const delay = backoffMs();
      setTimeout(connect, delay);
    };
  }

  const connection: EventConnection = {
    state: currentState,

    on<T extends FlowEventData>(type: FlowEventType, handler: EventTypeHandler<T>) {
      let handlers = eventHandlers.get(type);
      if (!handlers) {
        handlers = [];
        eventHandlers.set(type, handlers);
      }
      handlers.push(handler as EventTypeHandler);
    },

    onConnectionChange(handler: ConnectionChangeHandler) {
      connectionChangeHandlers.push(handler);
    },

    close() {
      closed = true;
      ws?.close();
    },
  };

  connect();
  return connection;
}
