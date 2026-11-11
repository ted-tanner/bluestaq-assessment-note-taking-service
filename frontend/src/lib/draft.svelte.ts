import { api, ApiError, json, message, type Note } from "./api";
import { session } from "./session.svelte";
export class Draft {
  note = $state<Note | null>(null);
  title = $state("");
  body = $state("");
  savedTitle = $state("");
  savedBody = $state("");
  folderId = $state<string | null>(null);
  savedFolderId = $state<string | null>(null);
  status = $state<
    | "saved"
    | "unsaved"
    | "saving"
    | "error"
    | "conflict"
    | "expired"
    | "uncertain"
  >("saved");
  editorKey = $state(0);
  error = $state("");
  latest = $state<Note | null>(null);
  private timer: ReturnType<typeof setTimeout> | undefined;
  private pending: Promise<boolean> | undefined;
  private disposed = false;
  constructor(
    note: Note | null,
    private onSaved: (note: Note, created: boolean) => void,
    initialFolder: string | null = null,
  ) {
    this.note = note;
    this.folderId = this.savedFolderId = note ? note.folder_id : initialFolder;
    this.title = this.savedTitle = note?.title ?? "";
    this.body = this.savedBody = note?.body ?? "";
  }
  get dirty() {
    return (
      this.title !== this.savedTitle ||
      this.body !== this.savedBody ||
      this.folderId !== this.savedFolderId
    );
  }
  get validation() {
    if ([...this.title].length > 255)
      return "The title is limited to 255 characters.";
    if (new TextEncoder().encode(this.body).length > 102400)
      return "The note is limited to 100 KiB. Shorten it before saving.";
    return "";
  }
  changed() {
    clearTimeout(this.timer);
    if (["conflict", "expired", "uncertain", "error"].includes(this.status))
      return;
    this.status = this.dirty ? "unsaved" : "saved";
    this.timer = setTimeout(() => void this.save(), 800);
  }
  save(): Promise<boolean> {
    clearTimeout(this.timer);
    if (this.pending) return this.pending;
    if (
      this.disposed ||
      ["conflict", "expired", "uncertain"].includes(this.status)
    )
      return Promise.resolve(false);
    this.pending = this.flush().finally(() => {
      this.pending = undefined;
    });
    return this.pending;
  }
  private async flush(): Promise<boolean> {
    while (this.dirty && !this.disposed) {
      if (this.validation) {
        this.error = this.validation;
        this.status = "error";
        return false;
      }
      if (!this.note && !this.title.trim() && !this.body.trim()) {
        this.status = "unsaved";
        return true;
      }
      const title = this.title,
        body = this.body,
        folder_id = this.folderId,
        existing = this.note;
      this.status = "saving";
      this.error = "";
      try {
        const result = await api<Note>(
          existing ? `/notes/${existing.id}` : "/notes",
          {
            method: existing ? "PATCH" : "POST",
            headers: existing ? { "If-Match": `"${existing.revision}"` } : {},
            body: json({ title, body, folder_id }),
          },
        );
        if (this.disposed) return false;
        this.note = result;
        this.savedTitle = title;
        this.savedBody = body;
        this.savedFolderId = folder_id;
        this.onSaved(result, !existing);
      } catch (e) {
        if (this.disposed) return false;
        this.error = message(e);
        if (e instanceof ApiError && e.status === 401) {
          this.status = "expired";
          session.expired = true;
        } else if (e instanceof ApiError && e.status === 412) {
          this.status = "conflict";
          try {
            this.latest = await api<Note>(`/notes/${existing!.id}`);
          } catch (loadError) {
            this.error = message(loadError);
          }
        } else if (!existing && (!(e instanceof ApiError) || e.status >= 500)) {
          this.status = "uncertain";
          this.error =
            "The connection ended before creation was confirmed. Check your library before creating another copy.";
        } else this.status = "error";
        return false;
      }
    }
    this.status = "saved";
    return !this.disposed;
  }
  resume() {
    this.status = this.dirty ? "unsaved" : "saved";
    this.error = "";
    return this.save();
  }
  loadLatest() {
    if (!this.latest) return;
    this.note = this.latest;
    this.folderId = this.savedFolderId = this.latest.folder_id;
    this.title = this.savedTitle = this.latest.title;
    this.body = this.savedBody = this.latest.body;
    this.latest = null;
    this.error = "";
    this.status = "saved";
    this.editorKey++;
  }
  replaceLatest() {
    if (!this.latest) return Promise.resolve(false);
    this.note = this.latest;
    this.latest = null;
    this.status = "unsaved";
    return this.save();
  }
  async copy() {
    // Only an explicit action creates a copy; ambiguous failures stay paused.
    this.note = null;
    this.savedTitle = "";
    this.savedBody = "";
    this.latest = null;
    this.status = "unsaved";
    return this.save();
  }
  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
  }
}
