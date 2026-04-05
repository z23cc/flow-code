type EventHandler = (data: unknown) => void;

export function connectEvents(onMessage: EventHandler): WebSocket {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${window.location.host}/api/v1/events`);

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      onMessage(data);
    } catch {
      console.warn("Failed to parse WS message:", event.data);
    }
  };

  ws.onerror = (err) => {
    console.error("WebSocket error:", err);
  };

  ws.onclose = () => {
    // Reconnect after 3 seconds
    setTimeout(() => connectEvents(onMessage), 3000);
  };

  return ws;
}
