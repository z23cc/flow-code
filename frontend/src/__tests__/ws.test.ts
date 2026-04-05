import { describe, it, expect, beforeEach } from "bun:test";
import type { ConnectionState } from "../lib/ws";

const instances: Array<{
  onopen: (() => void) | null;
  onclose: (() => void) | null;
  onmessage: ((e: { data: string }) => void) | null;
  onerror: (() => void) | null;
  closed: boolean;
  close: () => void;
  simulateOpen: () => void;
  simulateClose: () => void;
}> = [];

class MockWebSocket {
  onopen: (() => void) | null = null;
  onclose: (() => void) | null = null;
  onmessage: ((e: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(_url: string) {
    instances.push(this);
  }

  close() {
    this.closed = true;
  }

  simulateOpen() {
    this.onopen?.();
  }

  simulateClose() {
    this.onclose?.();
  }
}

globalThis.WebSocket = MockWebSocket as unknown as typeof WebSocket;

import { connectEvents } from "../lib/ws";

describe("WebSocket connection", () => {
  beforeEach(() => {
    instances.length = 0;
  });

  it("tracks connection state changes", () => {
    const states: ConnectionState[] = [];
    const conn = connectEvents();
    conn.onConnectionChange((s) => states.push(s));

    const ws = instances[0];
    ws.simulateOpen();
    ws.simulateClose();

    expect(states).toEqual(["connected", "reconnecting"]);
    conn.close();
  });

  it("starts in disconnected state and connects", () => {
    const conn = connectEvents();
    // Initial WebSocket created
    expect(instances).toHaveLength(1);

    // Simulate open
    instances[0].simulateOpen();
    expect(conn.state).toBe("connected");

    conn.close();
  });
});
