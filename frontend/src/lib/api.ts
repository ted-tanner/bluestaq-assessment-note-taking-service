export interface User {
  id: string;
  email: string;
  name: string;
}
export type Access = "owner" | "edit" | "view";
export interface ShareEntry {
  user: User;
  permission: "view" | "edit";
}
export interface Note {
  access: Access;
  owner: User;
  id: string;
  title: string;
  body: string;
  created_at: number;
  updated_at: number;
  revision: number;
  folder_id: string | null;
}
export interface Folder {
  access: Access;
  owner: User;
  id: string;
  name: string;
  parent_id: string | null;
  created_at: number;
  updated_at: number;
  revision: number;
}
export interface Challenge {
  challenge_id: string;
  purpose: "registration" | "signin" | "reset";
  expires_in: number;
}
export interface Results {
  items: Note[];
  has_more: boolean;
}
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}
let csrfPending: Promise<void> | undefined;
let refreshPending: Promise<void> | undefined;
function csrfCookie() {
  const cookies = document.cookie.split(";").map((c) => c.trim());
  const value =
    cookies.find((c) => c.startsWith("__Host-xsrf-token=")) ??
    cookies.find((c) => c.startsWith("xsrf-token="));
  return value ? decodeURIComponent(value.slice(value.indexOf("=") + 1)) : "";
}
async function ensureCsrf() {
  if (csrfCookie()) return;
  csrfPending ??= fetch("/api/auth/csrf-token", {
    credentials: "same-origin",
    cache: "no-store",
  })
    .then((r) => {
      if (!r.ok)
        throw new ApiError(
          r.status,
          "Could not establish a secure connection. Please retry.",
        );
    })
    .finally(() => {
      csrfPending = undefined;
    });
  await csrfPending;
  if (!csrfCookie())
    throw new ApiError(
      0,
      "Cookies are required. Check your browser settings and HTTPS configuration.",
    );
}
async function renew() {
  const work = async () => {
    // Another request or tab may have rotated the shared cookies while we waited.
    const me = await fetch("/api/user/me", {
      credentials: "same-origin",
      cache: "no-store",
    });
    if (me.ok) return;
    if (me.status !== 401)
      throw new ApiError(
        me.status,
        "Unable to check your session. Please retry.",
      );
    await api("/auth/refresh", { method: "POST" }, false);
  };
  refreshPending ??= (
    navigator.locks
      ? navigator.locks.request("bluestaq-session-refresh", work)
      : work()
  ).finally(() => {
    refreshPending = undefined;
  });
  await refreshPending;
}
export async function api<T>(
  path: string,
  options: RequestInit = {},
  protectedRequest = true,
): Promise<T> {
  const method = options.method ?? "GET";
  const unsafe = !["GET", "HEAD", "OPTIONS"].includes(method);
  const send = async () => {
    if (unsafe) await ensureCsrf();
    const headers = new Headers(options.headers);
    if (options.body) headers.set("Content-Type", "application/json");
    if (unsafe) headers.set("x-xsrf-token", csrfCookie());
    return fetch(`/api${path}`, {
      ...options,
      method,
      headers,
      credentials: "same-origin",
      cache: "no-store",
    });
  };
  let response = await send();
  if (response.status === 401 && protectedRequest) {
    await renew();
    response = await send();
  }
  if (!response.ok) {
    let message =
      response.status === 429
        ? "Too many attempts. Please wait a moment and try again."
        : "The request could not be completed. Please try again.";
    try {
      const data = await response.json();
      if (typeof data.error === "string") message = data.error;
    } catch {
      /* Never display untrusted HTML error pages. */
    }
    throw new ApiError(response.status, message);
  }
  return response.status === 204 ? (undefined as T) : response.json();
}
export function message(error: unknown): string {
  return error instanceof ApiError
    ? error.message
    : "Unable to connect. Check your connection and retry.";
}
export const json = (value: unknown) => JSON.stringify(value);
