import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { Draft } from "./draft.svelte";
import { api, ApiError, type Note } from "./api";
import { session } from "./session.svelte";
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  api: vi.fn(),
}));
const note: Note = {
  id: "note",
  access: "owner",
  owner: { id: "owner", email: "owner@test.test", name: "Owner" },
  folder_id: null,
  title: "before",
  body: "body",
  revision: 1,
  created_at: 1,
  updated_at: 1,
};
beforeEach(() => {
  vi.mocked(api).mockReset();
  session.user = { id: "owner", email: "owner@test.test", name: "Owner" };
  session.expired = false;
});
afterEach(() => vi.useRealTimers());
describe("autosave state", () => {
  it("debounces typing and keeps the most recent text during an in-flight save", async () => {
    vi.useFakeTimers();
    let resolve!: (value: Note) => void;
    vi.mocked(api)
      .mockImplementationOnce(
        () => new Promise((r) => (resolve = r)) as ReturnType<typeof api>,
      )
      .mockResolvedValueOnce({ ...note, revision: 3, title: "latest" });
    const saved = vi.fn();
    const draft = new Draft(note, saved);
    draft.title = "first";
    draft.changed();
    await vi.advanceTimersByTimeAsync(799);
    expect(api).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    draft.title = "latest";
    draft.changed();
    resolve({ ...note, title: "first", revision: 2 });
    await vi.advanceTimersByTimeAsync(1);
    await draft.save();
    expect(draft.title).toBe("latest");
    expect(draft.note?.revision).toBe(3);
    expect(draft.dirty).toBe(false);
    expect(api).toHaveBeenCalledTimes(2);
    expect(vi.mocked(api).mock.calls[1][1]?.headers).toEqual({
      "If-Match": '"2"',
    });
    draft.dispose();
  });
  it("retains failed edits and supports explicit retry", async () => {
    vi.mocked(api)
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce({ ...note, title: "retry", revision: 2 });
    const draft = new Draft(note, vi.fn());
    draft.title = "retry";
    expect(await draft.save()).toBe(false);
    expect(draft.dirty).toBe(true);
    expect(draft.status).toBe("error");
    expect(await draft.save()).toBe(true);
    draft.dispose();
  });
  it("pauses on conflicts and replaces only against the fetched revision", async () => {
    vi.mocked(api)
      .mockRejectedValueOnce(new ApiError(412, "changed"))
      .mockResolvedValueOnce({ ...note, title: "remote", revision: 4 })
      .mockResolvedValueOnce({ ...note, title: "local", revision: 5 });
    const draft = new Draft(note, vi.fn());
    draft.title = "local";
    await draft.save();
    expect(draft.status).toBe("conflict");
    expect(draft.title).toBe("local");
    expect(await draft.save()).toBe(false);
    await draft.replaceLatest();
    expect(vi.mocked(api).mock.calls[2][1]?.headers).toEqual({
      "If-Match": '"4"',
    });
    expect(draft.dirty).toBe(false);
    draft.dispose();
  });
  it("preserves drafts when a session expires and resumes after authentication", async () => {
    vi.mocked(api)
      .mockRejectedValueOnce(new ApiError(401, "expired"))
      .mockResolvedValueOnce({ ...note, body: "private draft", revision: 2 });
    const draft = new Draft(note, vi.fn());
    draft.body = "private draft";
    await draft.save();
    expect(session.expired).toBe(true);
    expect(draft.body).toBe("private draft");
    session.expired = false;
    await draft.resume();
    expect(draft.status).toBe("saved");
    draft.dispose();
  });
  it("never retries an ambiguous creation automatically", async () => {
    vi.mocked(api).mockRejectedValue(new TypeError("network error"));
    const draft = new Draft(null, vi.fn());
    draft.title = "new";
    await draft.save();
    expect(draft.status).toBe("uncertain");
    draft.body = "more";
    draft.changed();
    await draft.save();
    expect(api).toHaveBeenCalledTimes(1);
    draft.dispose();
  });
  it("enforces UTF-8 byte limits and does not create blank notes", async () => {
    const draft = new Draft(null, vi.fn());
    await draft.save();
    draft.body = "é".repeat(51201);
    expect(await draft.save()).toBe(false);
    expect(api).not.toHaveBeenCalled();
    draft.dispose();
  });
});

describe("folder assignments", () => {
  it("creates in the selected folder and moves to Unfiled with explicit null", async () => {
    vi.mocked(api).mockResolvedValueOnce({
      ...note,
      title: "New",
      folder_id: "folder",
    });
    const draft = new Draft(null, vi.fn(), "folder");
    draft.title = "New";
    expect(await draft.save()).toBe(true);
    expect(JSON.parse(vi.mocked(api).mock.calls[0][1]!.body as string)).toEqual(
      { title: "New", body: "", folder_id: "folder" },
    );
    draft.folderId = null;
    expect(draft.dirty).toBe(true);
    vi.mocked(api).mockResolvedValueOnce({
      ...note,
      title: "New",
      folder_id: null,
      revision: 2,
    });
    expect(await draft.save()).toBe(true);
    expect(
      JSON.parse(vi.mocked(api).mock.calls[1][1]!.body as string).folder_id,
    ).toBeNull();
    expect(draft.dirty).toBe(false);
    draft.dispose();
  });
  it("retains a conflicting move and adopts the latest folder only on load latest", async () => {
    const original = { ...note, folder_id: "original" };
    vi.mocked(api)
      .mockRejectedValueOnce(new ApiError(412, "changed"))
      .mockResolvedValueOnce({ ...original, folder_id: "remote", revision: 2 });
    const draft = new Draft(original, vi.fn());
    draft.folderId = "local";
    await draft.save();
    expect(draft.folderId).toBe("local");
    expect(draft.status).toBe("conflict");
    draft.loadLatest();
    expect(draft.folderId).toBe("remote");
    expect(draft.dirty).toBe(false);
    draft.dispose();
  });
});
