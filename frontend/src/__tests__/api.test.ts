import { describe, it, expect, mock, beforeEach, afterEach } from "bun:test";
import { ApiRequestError, apiFetch, apiPost } from "../lib/api";

describe("ApiRequestError", () => {
  it("parses server error JSON into status and message", () => {
    const err = new ApiRequestError(422, "Validation failed");
    expect(err.status).toBe(422);
    expect(err.serverMessage).toBe("Validation failed");
    expect(err.name).toBe("ApiRequestError");
    expect(err.message).toBe("Validation failed");
  });
});

describe("apiFetch", () => {
  const originalFetch = globalThis.fetch;

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("throws ApiRequestError with server error message on non-ok response", async () => {
    globalThis.fetch = mock(() =>
      Promise.resolve({
        ok: false,
        status: 400,
        statusText: "Bad Request",
        json: () => Promise.resolve({ error: "Invalid task ID" }),
      }),
    ) as unknown as typeof fetch;

    try {
      await apiFetch("/tasks");
      throw new Error("should have thrown");
    } catch (err) {
      expect(err).toBeInstanceOf(ApiRequestError);
      expect((err as ApiRequestError).status).toBe(400);
      expect((err as ApiRequestError).serverMessage).toBe("Invalid task ID");
    }
  });

  it("falls back to statusText when response body is not JSON", async () => {
    globalThis.fetch = mock(() =>
      Promise.resolve({
        ok: false,
        status: 500,
        statusText: "Internal Server Error",
        json: () => Promise.reject(new SyntaxError("not json")),
      }),
    ) as unknown as typeof fetch;

    try {
      await apiFetch("/tasks");
      throw new Error("should have thrown");
    } catch (err) {
      expect((err as ApiRequestError).serverMessage).toBe(
        "Internal Server Error",
      );
    }
  });

  it("returns parsed JSON on success", async () => {
    globalThis.fetch = mock(() =>
      Promise.resolve({
        ok: true,
        json: () => Promise.resolve({ count: 3, tasks: [] }),
      }),
    ) as unknown as typeof fetch;

    const result = await apiFetch<{ count: number }>("/tasks");
    expect(result.count).toBe(3);
  });
});

describe("apiPost", () => {
  const originalFetch = globalThis.fetch;

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("sends POST with JSON-stringified body", async () => {
    const mockFn = mock(() =>
      Promise.resolve({
        ok: true,
        json: () => Promise.resolve({ id: "task-1" }),
      }),
    );
    globalThis.fetch = mockFn as unknown as typeof fetch;

    await apiPost("/tasks/task-1/start", { force: true });

    expect(mockFn).toHaveBeenCalledTimes(1);
    const call = mockFn.mock.calls[0] as unknown as [string, RequestInit];
    const [url, opts] = call;
    expect(url).toBe("/api/v1/tasks/task-1/start");
    expect(opts.method).toBe("POST");
    expect(opts.body).toBe(JSON.stringify({ force: true }));
  });
});
