const API_BASE = "/api/v1";

export class ApiRequestError extends Error {
  status: number;
  serverMessage: string;

  constructor(status: number, serverMessage: string) {
    super(serverMessage);
    this.name = "ApiRequestError";
    this.status = status;
    this.serverMessage = serverMessage;
  }
}

export async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json", ...init?.headers },
    ...init,
  });
  if (!res.ok) {
    let serverMessage = res.statusText;
    try {
      const body = await res.json();
      if (body?.error) {
        serverMessage = body.error;
      }
    } catch {
      // response body wasn't JSON, keep statusText
    }
    throw new ApiRequestError(res.status, serverMessage);
  }
  return res.json();
}

export async function apiPost<T>(
  path: string,
  body?: unknown,
): Promise<T> {
  return apiFetch<T>(path, {
    method: "POST",
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
}

export async function apiDelete(path: string): Promise<void> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
  });
  if (!res.ok) {
    let serverMessage = res.statusText;
    try {
      const body = await res.json();
      if (body?.error) {
        serverMessage = body.error;
      }
    } catch {
      // response body wasn't JSON, keep statusText
    }
    throw new ApiRequestError(res.status, serverMessage);
  }
}

export const swrFetcher = <T>(path: string): Promise<T> => apiFetch<T>(path);
