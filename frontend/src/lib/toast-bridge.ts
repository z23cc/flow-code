import { toast } from "sonner";

interface FlowEvent {
  timestamp: string;
  event: {
    type: string;
    data: { task_id?: string; epic_id?: string; [key: string]: unknown };
  };
}

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

function handleEvent(raw: unknown): void {
  const ev = raw as FlowEvent;
  const { type, data } = ev.event;

  switch (type) {
    case "TaskCompleted":
      toast.success(`Task ${data.task_id ?? "unknown"} completed`);
      break;
    case "TaskFailed":
      toast.error(`Task ${data.task_id ?? "unknown"} failed`);
      break;
    case "EpicCompleted":
      toast.success("Epic completed", {
        description: `Epic ${data.epic_id ?? "unknown"} finished successfully`,
      });
      break;
    case "TaskStarted":
      toast.info(`Task ${data.task_id ?? "unknown"} started`);
      break;
    case "TaskBlocked":
      toast.warning(`Task ${data.task_id ?? "unknown"} blocked`);
      break;
    default:
      // Ignore other event types
      break;
  }
}

function connect(): void {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  ws = new WebSocket(`${proto}//${window.location.host}/api/v1/events`);

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      handleEvent(data);
    } catch {
      // Ignore non-JSON messages
    }
  };

  ws.onerror = () => {
    // Error logged by browser; reconnect handled by onclose
  };

  ws.onclose = () => {
    ws = null;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(connect, 3000);
  };
}

/** Start the toast bridge. Call once at app init. Returns a cleanup function. */
export function startToastBridge(): () => void {
  connect();
  return () => {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.onclose = null; // Prevent reconnect on intentional close
      ws.close();
      ws = null;
    }
  };
}
