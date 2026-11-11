import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { api, ApiError } from "./api";
const response = (status = 200, data: unknown = {}) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { "Content-Type": "application/json" },
  });
beforeEach(() => {
  document.cookie = "xsrf-token=; Max-Age=0; path=/";
});
afterEach(() => vi.unstubAllGlobals());
describe("API security and session recovery", () => {
  it("initializes CSRF before unsafe public requests without refreshing invalid credentials", async () => {
    const fetcher = vi.fn(async (url: string, _options?: RequestInit) => {
      if (url.endsWith("/csrf-token")) {
        document.cookie = "xsrf-token=csrf-test; path=/";
        return response();
      }
      return response(401, { error: "Invalid credentials" });
    });
    vi.stubGlobal("fetch", fetcher);
    await expect(
      api("/auth/sign-in", { method: "POST", body: "{}" }, false),
    ).rejects.toMatchObject({ status: 401 });
    expect(fetcher).toHaveBeenCalledTimes(2);
    const options = fetcher.mock.calls[1][1] as RequestInit;
    expect(new Headers(options.headers).get("x-xsrf-token")).toBe("csrf-test");
    expect(options.credentials).toBe("same-origin");
  });
  it("shares one refresh between concurrent expired requests and retries only once", async () => {
    document.cookie = "xsrf-token=test; path=/";
    let active = false,
      refreshes = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string, _options?: RequestInit) => {
        if (url.endsWith("/auth/refresh")) {
          refreshes++;
          await new Promise((r) => setTimeout(r, 10));
          active = true;
          return response();
        }
        return response(
          active ? 200 : 401,
          active ? { id: "me" } : { error: "expired" },
        );
      }),
    );
    await Promise.all([api("/notes"), api("/notes")]);
    expect(refreshes).toBe(1);
  });
  it("does not display arbitrary HTML returned by a failed endpoint", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () => new Response("<script>bad()</script>", { status: 502 }),
      ),
    );
    await expect(api("/notes")).rejects.toThrow(
      "The request could not be completed",
    );
  });
  it("does not loop when refresh is rejected", async () => {
    document.cookie = "xsrf-token=test; path=/";
    const fetcher = vi.fn(async () => response(401));
    vi.stubGlobal("fetch", fetcher);
    await expect(api("/notes")).rejects.toBeInstanceOf(ApiError);
    expect(fetcher).toHaveBeenCalledTimes(3);
  });
});
