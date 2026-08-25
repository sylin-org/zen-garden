// Thin fetch wrapper. All paths are relative — Vite proxy handles routing.

export class ApiError extends Error {
  code: string;
  details?: unknown;
  status?: number;

  constructor(code: string, message: string, details?: unknown, status?: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.details = details;
    this.status = status;
  }
}

async function handleResponse<T>(res: Response): Promise<T> {
  if (res.status === 304) {
    throw new ApiError("not_modified", "Not modified", undefined, 304);
  }
  const body = await res.json();
  if (!res.ok) {
    const err = body?.error ?? {};
    throw new ApiError(
      err.code ?? "unknown",
      err.message ?? res.statusText,
      err.details,
      res.status,
    );
  }
  return body as T;
}

export async function get<T>(path: string, etag?: string): Promise<T> {
  const headers: Record<string, string> = {};
  if (etag) headers["If-None-Match"] = etag;
  const res = await fetch(path, { headers });
  return handleResponse<T>(res);
}

export async function post<T>(
  path: string,
  body: unknown,
  idempotencyKey?: string,
): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (idempotencyKey) headers["idempotency-key"] = idempotencyKey;
  const res = await fetch(path, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  });
  return handleResponse<T>(res);
}

export async function put<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  return handleResponse<T>(res);
}

export async function del(path: string): Promise<void> {
  const res = await fetch(path, { method: "DELETE" });
  if (!res.ok && res.status !== 204) {
    const body = await res.json().catch(() => ({}));
    const err = body?.error ?? {};
    throw new ApiError(
      err.code ?? "unknown",
      err.message ?? res.statusText,
      err.details,
      res.status,
    );
  }
}

export async function upload(path: string, file: File): Promise<unknown> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": file.type || "application/octet-stream" },
    body: file,
  });
  return handleResponse(res);
}

/** POST dispatch that may return JSON or SSE stream. */
export async function dispatch(
  path: string,
  body: unknown,
  idempotencyKey: string,
): Promise<Response> {
  return fetch(path, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "idempotency-key": idempotencyKey,
    },
    body: JSON.stringify(body),
  });
}
