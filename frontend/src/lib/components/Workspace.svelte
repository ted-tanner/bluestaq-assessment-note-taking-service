<script lang="ts">
  import { autosize } from "#lib/autosize.js";
  import { modal } from "#lib/modal.js";
  import { onMount, tick } from "svelte";
  import { goto, beforeNavigate, afterNavigate } from "$app/navigation";
  import { page, navigating } from "$app/state";
  import {
    api,
    ApiError,
    message,
    type Note,
    type Results,
    type Folder,
  } from "#lib/api.js";
  import { Draft } from "#lib/draft.svelte.js";
  import { session } from "#lib/session.svelte.js";
  import Markdown from "./Markdown.svelte";
  import AuthForm from "./AuthForm.svelte";
  import ActionMenu from "./ActionMenu.svelte";
  import FolderNavigation from "./FolderNavigation.svelte";
  import { descendants, folderPath, folderRows } from "#lib/folders.js";
  import ShareDialog from "./ShareDialog.svelte";
  import Profile from "./Profile.svelte";
  import PasswordChange from "./PasswordChange.svelte";
  import {
    NotebookPen,
    Plus,
    Search,
    LogOut,
    KeyRound,
    Sun,
    Moon,
    Monitor,
    ArrowLeft,
    PenLine,
    BookOpen,
    Trash2,
    FileText,
    Check,
    Cloud,
    AlertCircle,
    ArrowRight,
    FolderInput,
    PanelLeftClose,
    PanelLeftOpen,
    Share2,
    UserRound,
  } from "@lucide/svelte";
  let sidebarCollapsed = $state(false);
  async function toggleSidebar() {
    sidebarCollapsed = !sidebarCollapsed;
    await tick();
    document
      .getElementById(
        sidebarCollapsed && selected ? "sidebar-reopen" : "sidebar-toggle",
      )
      ?.focus();
  }
  let profileOpen = $state(false);
  let shareTarget = $state<{
    kind: "notes" | "folders";
    id: string;
    title: string;
  }>();
  $effect(() => {
    if (session.user && !session.user.name?.trim()) profileOpen = true;
  });
  function sharedChanged() {
    channel?.postMessage({ type: "notes-changed" });
    void loadFolders();
    void search();
  }
  function noteLocation(note: Note) {
    if (
      note.access !== "owner" &&
      note.folder_id &&
      !folders.some((f) => f.id === note.folder_id)
    )
      return "Shared note";
    return folderPath(folders, note.folder_id);
  }
  let query = $state("");
  let folders = $state<Folder[]>([]);
  let folderError = $state("");
  const location = $derived(page.url.searchParams.get("folder") ?? "");
  const currentFolder = $derived(folders.find((f) => f.id === location));
  type FolderAction = {
    kind: "create" | "rename" | "move" | "delete";
    folder?: Folder;
  };
  let folderAction = $state<FolderAction>();
  let folderName = $state("");
  let destination = $state("");
  let actionError = $state("");
  let actionBusy = $state(false);
  let noteMoveOpen = $state(false);
  const destinations = $derived(
    folderRows(folders.filter((f) => f.access === "owner")).filter(
      (row) =>
        !folderAction?.folder ||
        !descendants(folders, folderAction.folder.id).has(row.folder.id),
    ),
  );
  function contextHref(path: string, folder = location) {
    const params = new URLSearchParams();
    if (folder) params.set("folder", folder);
    if (query) params.set("q", query);
    return path + (params.size ? `?${params}` : "");
  }
  function folderHref(id: string) {
    return contextHref("/notes", id);
  }
  let pendingNavigationHref: string | undefined;
  function updateQuery(value: string) {
    if (page.url.port === "15200") console.debug("qa-search input", value, page.url.pathname, pendingNavigationHref);
    query = value;
    const url = new URL(pendingNavigationHref ?? page.url.href);
    if (query) url.searchParams.set("q", query);
    else url.searchParams.delete("q");
    // This route has no load functions. A normal router update keeps the URL,
    // history, and page state together without remounting the current draft.
    void goto(url, { replace: true, reset: false });
  }
  afterNavigate((nav) => {
    if (page.url.port === "15200") console.debug("qa-search after", nav.shallow, page.url.href, query);
    pendingNavigationHref = undefined;
  });
  $effect(() => {
    query = (page.shallow?.url ?? page.url).searchParams.get("q") ?? "";
  });
  let folderGeneration = 0;
  async function loadFolders() {
    const generation = ++folderGeneration;
    try {
      const result = await api<Folder[]>("/folders");
      if (generation !== folderGeneration) return;
      folders = result;
      folderError = "";
    } catch (e) {
      if (generation !== folderGeneration) return;
      folderError = message(e);
      if (e instanceof ApiError && e.status === 401) session.expired = true;
    }
  }
  $effect(() => {
    const owner = session.user?.id;
    location;
    if (owner) void loadFolders();
  });
  function openFolder(kind: FolderAction["kind"], folder = currentFolder) {
    folderAction = { kind, folder: kind === "create" ? undefined : folder };
    folderName = kind === "rename" ? (folder?.name ?? "") : "";
    destination =
      kind === "create" ? (currentFolder?.id ?? "") : (folder?.parent_id ?? "");
    actionError = "";
  }
  async function submitFolder(event: SubmitEvent) {
    event.preventDefault();
    const action = folderAction;
    if (!action || actionBusy) return;
    actionBusy = true;
    actionError = "";
    try {
      const removed =
        action.kind === "delete" && action.folder
          ? descendants(folders, action.folder.id)
          : new Set<string>();
      const affected =
        !!draft &&
        (removed.has(draft.folderId ?? "") ||
          removed.has(draft.note?.folder_id ?? ""));
      if (affected && !(await draft!.save())) {
        actionError =
          "Save or resolve your open note before deleting this folder.";
        return;
      }
      const body =
        action.kind === "create"
          ? { name: folderName, parent_id: destination || null }
          : action.kind === "rename"
            ? { name: folderName }
            : { parent_id: destination || null };
      await api(
        action.kind === "create" ? "/folders" : `/folders/${action.folder!.id}`,
        {
          method:
            action.kind === "create"
              ? "POST"
              : action.kind === "delete"
                ? "DELETE"
                : "PATCH",
          headers: action.folder
            ? { "If-Match": `"${action.folder.revision}"` }
            : {},
          ...(action.kind === "delete" ? {} : { body: JSON.stringify(body) }),
        },
      );
      if (action.kind === "delete" && (affected || removed.has(location))) {
        if (affected) {
          draft?.dispose();
          draft = undefined;
        }
        bypass = true;
        try {
          await goto(
            contextHref(
              affected ? "/notes" : page.url.pathname,
              action.folder!.parent_id ?? "",
            ),
          );
        } finally {
          bypass = false;
        }
      }
      await loadFolders();
      await search();
      folderAction = undefined;
      channel?.postMessage({ type: "notes-changed" });
    } catch (e) {
      actionError =
        e instanceof ApiError && e.status === 412
          ? "This folder changed elsewhere. Close this dialog and try again with the refreshed folder."
          : message(e);
      if (e instanceof ApiError && e.status === 401) session.expired = true;
      if (e instanceof ApiError && [404, 412].includes(e.status))
        await loadFolders();
    } finally {
      actionBusy = false;
    }
  }
  async function moveNote(event: SubmitEvent) {
    event.preventDefault();
    if (!draft || actionBusy) return;
    actionBusy = true;
    actionError = "";
    try {
      draft.folderId = destination || null;
      draft.changed();
      if (await draft.save()) {
        noteMoveOpen = false;
        await search();
      } else
        actionError =
          draft.error ||
          "Resolve your note’s unsaved changes before continuing.";
    } finally {
      actionBusy = false;
    }
  }
  let notes = $state<Note[]>([]);
  let hasMore = $state(false);
  let searching = $state(true);
  let listError = $state("");
  let noteError = $state("");
  let loading = $state(false);
  let draft = $state.raw<Draft>();
  const canCreate = $derived(
    location !== "shared" && currentFolder?.access !== "view",
  );
  const mayEdit = $derived(
    draft?.note
      ? draft.note.access !== "view"
      : currentFolder?.access !== "view",
  );
  const ownsNote = $derived(draft?.note?.access === "owner");
  let selected = $state("");
  let editing = $state(false);
  let passwordOpen = $state(false);
  let passwordNotice = $state("");
  let deleteOpen = $state(false);
  let deleting = $state(false);
  let deleteError = $state("");
  let leaveTarget = $state("");
  let leaving = $state(false);
  let bypass = false;
  let theme = $state<"system" | "light" | "dark">("system");
  let searchController: AbortController | undefined;
  let searchGeneration = 0;
  let loadGeneration = 0;
  let channel: BroadcastChannel | undefined;
  const date = (timestamp: number) =>
    new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    }).format(new Date(timestamp * 1000));
  const wordCount = $derived(
    draft?.body.trim().split(/\s+/).filter(Boolean).length ?? 0,
  );
  const status = $derived(
    draft?.validation ||
      (draft?.status === "saving"
        ? "Saving…"
        : draft?.status === "saved"
          ? "All changes saved"
          : draft?.status === "conflict"
            ? "Conflicting changes"
            : draft?.status === "error" || draft?.status === "uncertain"
              ? "Not saved"
              : draft?.status === "expired"
                ? "Sign in to save"
                : "Unsaved changes"),
  );
  async function search(more = false) {
    searchController?.abort();
    searchController = new AbortController();
    const generation = ++searchGeneration;
    const requestQuery = query;
    const requestLocation = location;
    const offset = more ? notes.length : 0;
    searching = true;
    listError = "";
    try {
      const params = new URLSearchParams({
        q: query,
        limit: "50",
        offset: String(offset),
      });
      if (location === "shared") params.set("folder_id", "shared");
      const result = await api<Results>(`/notes/search?${params}`, {
        signal: searchController.signal,
      });
      if (
        generation !== searchGeneration ||
        requestQuery !== query ||
        requestLocation !== location
      )
        return;
      notes = more
        ? [
            ...notes,
            ...result.items.filter(
              (n) => !notes.some((old) => old.id === n.id),
            ),
          ]
        : result.items;
      hasMore = result.has_more;
    } catch (e) {
      if (
        generation === searchGeneration &&
        !(e instanceof DOMException && e.name === "AbortError")
      ) {
        listError =
          e instanceof ApiError && e.status === 404 && location
            ? "This folder no longer exists or is unavailable. Choose All notes or another folder."
            : message(e);
        if (e instanceof ApiError && e.status === 401) session.expired = true;
      }
    } finally {
      if (generation === searchGeneration) searching = false;
    }
  }
  $effect(() => {
    query;
    location;
    searchController?.abort();
    searchGeneration++;
    searching = true;
    const timer = setTimeout(() => void search(), 250);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    const path = page.params.path ?? "";
    const parts = path.split("/");
    const id = parts[0];
    editing = id === "new" || parts[1] === "edit";
    if (id === selected || (draft?.note?.id === id && selected === "new")) {
      selected = id;
      return;
    }
    selected = id;
    noteError = "";
    const generation = ++loadGeneration;
    draft?.dispose();
    draft = undefined;
    if (!id) {
      loading = false;
      return;
    }
    if (id === "new") {
      draft = new Draft(
        null,
        saved,
        location && !["root", "shared"].includes(location) ? location : null,
      );
      loading = false;
      return;
    }
    loading = true;
    void api<Note>(`/notes/${encodeURIComponent(id)}`)
      .then((note) => {
        if (generation === loadGeneration) draft = new Draft(note, saved);
      })
      .catch((e) => {
        if (generation === loadGeneration) {
          noteError =
            e instanceof ApiError && e.status === 404
              ? "This note could not be found. It may have been deleted."
              : message(e);
          if (e instanceof ApiError && e.status === 401) session.expired = true;
        }
      })
      .finally(() => {
        if (generation === loadGeneration) loading = false;
      });
  });
  function saved(note: Note, created: boolean) {
    void search();
    channel?.postMessage({ type: "notes-changed" });
    if (created) {
      bypass = true;
      void goto(contextHref(`/notes/${note.id}/edit`), {
        replace: true,
      }).finally(() => {
        bypass = false;
      });
    }
  }
  beforeNavigate((nav) => {
    if (page.url.port === "15200") console.debug("qa-search before", nav.shallow, nav.to?.url.href, query);
    if (!nav.shallow) pendingNavigationHref = nav.to?.url.href;
    if (bypass || !draft?.dirty) return;
    if (!nav.willUnload && nav.to?.url.pathname === page.url.pathname) return;
    if (nav.willUnload) {
      nav.cancel();
      return;
    }
    if (
      nav.to?.url &&
      draft.note &&
      nav.to.url.pathname === `/notes/${draft.note.id}/edit`
    )
      return;
    nav.cancel();
    const href = nav.to?.url.href;
    if (href) void navigateAfterSave(href);
  });
  async function navigateAfterSave(href: string) {
    if (leaving) return;
    leaving = true;
    const ok = await draft?.save();
    if (ok) {
      bypass = true;
      await goto(href);
      bypass = false;
    } else leaveTarget = href;
    leaving = false;
  }
  async function discard() {
    draft?.dispose();
    draft = undefined;
    const target = leaveTarget;
    leaveTarget = "";
    bypass = true;
    await goto(target);
    bypass = false;
  }
  async function logout() {
    if (draft?.dirty && !(await draft.save())) {
      leaveTarget = "logout";
      return;
    }
    try {
      await api("/auth/logout", { method: "POST" }, false);
      channel?.postMessage({ type: "logout" });
      await clearAndExit();
    } catch (e) {
      listError = `Sign out failed. ${message(e)}`;
    }
  }
  async function clearAndExit() {
    draft?.dispose();
    draft = undefined;
    notes = [];
    query = "";
    session.user = null;
    session.expired = false;
    bypass = true;
    await goto("/sign-in", { replace: true });
    bypass = false;
  }
  async function deleteNote() {
    if (!draft?.note || deleting) return;
    deleting = true;
    deleteError = "";
    try {
      await api(`/notes/${draft.note.id}`, {
        method: "DELETE",
        headers: { "If-Match": `"${draft.note.revision}"` },
      });
      draft.dispose();
      draft = undefined;
      deleteOpen = false;
      channel?.postMessage({ type: "notes-changed" });
      bypass = true;
      await goto(contextHref("/notes"));
      bypass = false;
      await search();
    } catch (e) {
      deleteError =
        e instanceof ApiError && e.status === 412
          ? "This note changed elsewhere. Close this dialog and reload it before deleting."
          : message(e);
      if (e instanceof ApiError && e.status === 401) {
        deleteOpen = false;
        session.expired = true;
      }
    } finally {
      deleting = false;
    }
  }
  async function retryLoad() {
    selected = "";
    await goto(page.url.pathname, { invalidateAll: true });
  }
  function cycleTheme() {
    theme =
      theme === "system" ? "light" : theme === "light" ? "dark" : "system";
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("bluestaq-theme", theme);
  }
  onMount(() => {
    const stored = localStorage.getItem("bluestaq-theme");
    if (stored === "light" || stored === "dark") theme = stored;
    document.documentElement.dataset.theme = theme;
    channel = new BroadcastChannel("bluestaq-session");
    channel.onmessage = (e) => {
      if (e.data.type === "logout") {
        if (draft?.dirty) {
          session.expired = true;
          if (draft) draft.status = "expired";
        } else void clearAndExit();
      }
      if (e.data.type === "account" && e.data.id !== session.user?.id)
        void clearAndExit();
      if (e.data.type === "notes-changed") {
        void loadFolders();
        void search();
      }
    };
    return () => {
      draft?.dispose();
      searchController?.abort();
      channel?.close();
    };
  });
  function shortcut(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      if (draft && editing && mayEdit) void draft.save();
    }
  }
</script>

<svelte:head
  ><title>{draft?.title || "Your notes"} · Bluestaq Notes</title></svelte:head
>
<svelte:window onkeydown={shortcut} />
<div
  class="workspace"
  class:has-selection={!!selected}
  class:sidebar-collapsed={sidebarCollapsed}
  inert={session.expired}
>
  <aside class="library" id="note-library">
    <a class="brand" href="/notes"
      ><span class="brand-mark"><NotebookPen size={22} /></span> bluestaq<span
        class="brand-light">notes</span
      ></a
    >
    <div class="library-heading">
      <div>
        <p class="eyebrow">YOUR WORKSPACE</p>
        <div class="library-title">
          <h1>
            Notes<span class="count">{notes.length}{hasMore ? "+" : ""}</span>
          </h1>
        </div>
      </div>
      <div class="library-controls">
        {#if canCreate}<a
            class="button primary new-note"
            href={contextHref("/notes/new")}
            aria-label="New note"><Plus size={18} /><span>New note</span></a
          >{/if}
        <button
          id="sidebar-toggle"
          class="icon-button"
          aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!sidebarCollapsed}
          aria-controls={selected ? "note-library" : "folder-sidebar"}
          onclick={toggleSidebar}
        >
          {#if sidebarCollapsed}<PanelLeftOpen
              size={19}
            />{:else}<PanelLeftClose size={19} />{/if}
        </button>
      </div>
    </div>
    <div class="browser-body unified-browser">
      <div class="search-field">
        <Search size={17} /><input
          aria-label="Search your notes"
          placeholder="Search your notes…"
          disabled={leaving ||
            (!!navigating.to &&
              navigating.to.url.pathname !== page.url.pathname)}
          value={query}
          oninput={(e) => updateQuery(e.currentTarget.value)}
          maxlength="256"
        />{#if query}<button
            class="icon-button"
            aria-label="Clear search"
            onclick={() => updateQuery("")}>×</button
          >{/if}
      </div>
      {#if currentFolder}
        <div class="tree-context">
          <span>{folderPath(folders, currentFolder.id)}</span>
          {#if currentFolder.access !== "view"}<ActionMenu
              label="Folder actions"
            >
              <button onclick={() => openFolder("create")}>New subfolder</button
              >
              <button onclick={() => openFolder("rename")}>Rename folder</button
              >
              {#if currentFolder.access === "owner"}<button
                  onclick={() =>
                    (shareTarget = {
                      kind: "folders",
                      id: currentFolder!.id,
                      title: currentFolder!.name,
                    })}>Share folder</button
                >
                <button onclick={() => openFolder("move")}>Move folder</button>
                <button class="destructive" onclick={() => openFolder("delete")}
                  >Delete folder</button
                >{/if}
            </ActionMenu>{/if}
        </div>
      {/if}
      <div class="tree-content" aria-busy={searching}>
        <FolderNavigation
          folders={location === "shared"
            ? folders.filter((f) => f.access !== "owner")
            : folders}
          {notes}
          {query}
          {selected}
          {location}
          href={folderHref}
          noteHref={(note) => contextHref(`/notes/${note.id}`)}
          oncreate={() => openFolder("create")}
          {canCreate}
          error={folderError}
          onretry={() => void loadFolders()}
        />
        {#if searching}<p class="tree-status" role="status">Searching…</p>{/if}
        {#if listError}
          <div class="list-notice">
            <p role="alert">{listError}</p>
            <button onclick={() => search()}>Retry</button>
          </div>
        {:else if !notes.length && !searching && ((location === "shared" ? !folders.some((f) => f.access !== "owner") : !folders.length) || !!query.trim())}
          <div class="list-notice">
            <FileText size={28} />
            <h3>
              {query.trim()
                ? "No notes found"
                : location === "shared"
                  ? "Nothing shared with you yet"
                  : "A fresh start"}
            </h3>
            <p>
              {query.trim()
                ? "Try a different word or a shorter search."
                : location === "shared"
                  ? "Notes and folders shared with you will appear here."
                  : "Your first note is just a click away."}
            </p>
            {#if !query.trim() && canCreate}<a href={contextHref("/notes/new")}
                >Write your first note <ArrowRight size={14} /></a
              >{/if}
          </div>
        {/if}
        {#if hasMore && !listError}<button
            class="load-more"
            disabled={searching}
            onclick={() => search(true)}
            >{searching ? "Loading…" : "Load more notes"}</button
          >{/if}
      </div>
    </div>
    <footer class="library-footer">
      <div class="account-avatar">{session.user?.email[0].toUpperCase()}</div>
      <span class="account-email" title={session.user?.email}
        >{session.user?.name || session.user?.email}<small
          >{session.user?.email}</small
        ></span
      ><button
        class="icon-button"
        title={`Theme: ${theme}`}
        aria-label={`Theme: ${theme}. Change theme`}
        onclick={cycleTheme}
        >{#if theme === "dark"}<Moon
            size={17}
          />{:else if theme === "light"}<Sun size={17} />{:else}<Monitor
            size={17}
          />{/if}</button
      ><button
        class="icon-button"
        aria-label="Edit profile"
        title="Edit profile"
        onclick={() => (profileOpen = true)}><UserRound size={17} /></button
      ><button
        class="icon-button"
        title="Change password"
        aria-label="Change password"
        onclick={() => {
          passwordNotice = "";
          passwordOpen = true;
        }}><KeyRound size={17} /></button
      ><button
        class="icon-button"
        title="Sign out"
        aria-label="Sign out"
        onclick={logout}><LogOut size={17} /></button
      >
    </footer>
  </aside>
  <main class="note-area" id="main-content">
    {#if selected && sidebarCollapsed}
      <button
        id="sidebar-reopen"
        class="icon-button sidebar-reopen"
        aria-label="Expand sidebar"
        title="Expand sidebar"
        aria-expanded="false"
        aria-controls="note-library"
        onclick={toggleSidebar}><PanelLeftOpen size={19} /></button
      >
    {/if}
    {#if !selected}<div class="empty-workspace">
        <div class="empty-art">
          <NotebookPen size={46} strokeWidth={1.3} /><span class="art-dot one"
          ></span><span class="art-dot two"></span>
        </div>
        <p class="eyebrow">A LITTLE CLARITY STARTS HERE</p>
        <h2>Make space for your next idea.</h2>
        <p>Select a note to settle back in,<br />or start with a clean page.</p>
        <a class="button primary" href={contextHref("/notes/new")}
          ><Plus size={18} /> Create a note</a
        ><span class="empty-shortcut">Simple notes. Yours to shape.</span>
      </div>
    {:else if loading}<div class="empty-workspace" role="status">
        <Cloud size={30} />
        <p>Opening your note…</p>
      </div>
    {:else if noteError}<div class="empty-workspace">
        <AlertCircle size={30} />
        <h2>Unable to open note</h2>
        <p role="alert">{noteError}</p>
        <div class="actions">
          <a class="button" href="/notes">Back to notes</a><button
            onclick={retryLoad}>Retry</button
          >
        </div>
      </div>
    {:else if draft}
      <header class="note-topbar">
        <a class="back-link" href={contextHref("/notes")}
          ><ArrowLeft size={17} /><span
            >{currentFolder?.name ??
              (location === "root" ? "Top level" : "All notes")}</span
          ></a
        ><span class="note-location"
          ><FileText size={14} />
          {draft?.note
            ? noteLocation(draft.note)
            : draft
              ? folderPath(folders, draft.folderId)
              : "Personal notes"}</span
        >
        <div class="note-actions">
          {#if draft.note}{#if ownsNote}<button
                class="icon-button"
                aria-label="Share note"
                title="Share note"
                onclick={() =>
                  (shareTarget = {
                    kind: "notes",
                    id: draft!.note!.id,
                    title: draft!.title || "Untitled note",
                  })}><Share2 size={17} /></button
              ><button
                class="icon-button"
                aria-label="Move note"
                title="Move note"
                onclick={() => {
                  destination = draft?.folderId ?? "";
                  actionError = "";
                  noteMoveOpen = true;
                }}><FolderInput size={17} /></button
              >{/if}{#if mayEdit}<a
                class="button quiet"
                href={contextHref(
                  `/notes/${draft.note.id}${editing ? "" : "/edit"}`,
                )}
                >{#if editing}<BookOpen size={16} /> Read{:else}<PenLine
                    size={16}
                  /> Edit note{/if}</a
              >{/if}{#if ownsNote}<button
                class="icon-button"
                aria-label="Delete note"
                title="Delete note"
                onclick={async () => {
                  if (await draft?.save()) {
                    deleteError = "";
                    deleteOpen = true;
                  }
                }}><Trash2 size={17} /></button
              >{/if}{/if}
        </div>
      </header>
      <article class="note-document">
        {#if draft.note && !ownsNote}<p class="shared-banner">
            Shared by {draft.note.owner.name || draft.note.owner.email} · {mayEdit
              ? "Can edit"
              : "Can view"}
          </p>{/if}
        <div class="document-meta">
          <span class="document-type"
            ><FileText size={13} />
            {editing ? "YOUR NOTE" : "PERSONAL NOTE"}</span
          ><span class="save-status" role="status"
            >{#if draft.status === "saved"}<Check
                size={14}
              />{:else if draft.status === "error" || draft.status === "conflict"}<AlertCircle
                size={14}
              />{:else}<Cloud size={14} />{/if}{editing
              ? status
              : draft.note
                ? `Updated ${date(draft.note.updated_at)}`
                : ""}</span
          >
        </div>
        {#if editing && mayEdit}<textarea
            class="note-title"
            rows="1"
            use:autosize={draft.title}
            aria-label="Note title"
            placeholder="Untitled note"
            value={draft.title}
            oninput={(e) => {
              draft!.title = e.currentTarget.value;
              draft!.changed();
            }}></textarea>
        {:else}<h1 class="read-title">{draft.title || "Untitled note"}</h1>{/if}
        <div class="document-rule"></div>
        {#if draft.error || draft.validation}<div
            class="notice danger"
            role="alert"
          >
            <p>{draft.validation || draft.error}</p>
            {#if draft.status === "error"}<button onclick={() => draft?.save()}
                >Retry save</button
              >{:else if draft.status === "uncertain"}<a href="/notes"
                >Check library</a
              >{/if}
          </div>{/if}
        {#if draft.status === "conflict"}<section
            class="conflict-panel"
            aria-label="Resolve conflicting changes"
          >
            <h2>This note changed elsewhere</h2>
            <p>
              Your draft is safe here. Compare it with the latest saved version
              before continuing.
            </p>
            {#if draft.latest}<details open>
                <summary
                  >Latest saved version: {draft.latest.title ||
                    "Untitled note"}</summary
                ><Markdown body={draft.latest.body} />
              </details>
              <div class="actions">
                <button onclick={() => draft?.loadLatest()}>Load latest</button
                ><button onclick={() => draft?.copy()}
                  >Save draft as new note</button
                ><button class="primary" onclick={() => draft?.replaceLatest()}
                  >Replace latest with draft</button
                >
              </div>{:else}<button
                onclick={async () => {
                  try {
                    draft!.latest = await api<Note>(
                      `/notes/${draft!.note!.id}`,
                    );
                  } catch (e) {
                    draft!.error = message(e);
                  }
                }}>Retry loading latest</button
              >{/if}
          </section>{/if}
        {#if editing && mayEdit}{#key draft}{#key draft.editorKey}
              {#await import("./Editor.svelte")}<p role="status">
                  Loading editor…
                </p>
              {:then module}<module.default
                  body={draft.body}
                  onchange={(body) => {
                    draft!.body = body;
                    draft!.changed();
                  }}
                />
              {:catch}
                <p class="notice" role="alert">
                  The rich text editor could not load. You can keep editing in
                  Markdown below.
                </p>
                <textarea
                  class="markdown-source"
                  aria-label="Markdown source"
                  value={draft.body}
                  oninput={(e) => {
                    draft!.body = e.currentTarget.value;
                    draft!.changed();
                  }}
                  spellcheck="false"></textarea>
              {/await}{/key}{/key}
        {:else if draft.body}<Markdown body={draft.body} />{:else}<p
            class="blank-note"
          >
            This page is waiting for a thought.
          </p>{/if}
        <footer class="document-footer">
          <span>{wordCount} {wordCount === 1 ? "word" : "words"}</span><span
            >{editing
              ? "Markdown supported · ⌘ / Ctrl S to save"
              : "A little space for what matters."}</span
          >
        </footer>
      </article>
    {/if}
  </main>
</div>
{#if folderAction}<dialog
    class="modal"
    use:modal
    aria-labelledby="folder-dialog-title"
    oncancel={(e) => {
      e.preventDefault();
      if (!actionBusy) folderAction = undefined;
    }}
  >
    <form onsubmit={submitFolder}>
      <h2 id="folder-dialog-title">
        {folderAction.kind === "create"
          ? "New folder"
          : folderAction.kind === "rename"
            ? "Rename folder"
            : folderAction.kind === "move"
              ? "Move folder"
              : "Delete folder?"}
      </h2>
      {#if folderAction.kind === "delete"}
        <p>
          “{folderAction.folder?.name}” and all nested folders and notes will be
          permanently deleted. This cannot be undone.
        </p>
      {:else}
        {#if folderAction.kind !== "move"}<label
            >Folder name<input
              aria-label="Folder name"
              bind:value={folderName}
              required
              maxlength="255"
            /></label
          >{/if}
        {#if folderAction.kind !== "rename"}<label
            >Parent folder<select
              aria-label="Parent folder"
              bind:value={destination}
              ><option value="">Root</option
              >{#each destinations as { folder, depth }}<option
                  value={folder.id}
                  >{"— ".repeat(depth)}{folderPath(folders, folder.id)}</option
                >{/each}</select
            ></label
          >{/if}
      {/if}
      {#if actionError}<p role="alert" class="notice danger">
          {actionError}
        </p>{/if}
      <div class="actions">
        <button
          type="button"
          disabled={actionBusy}
          onclick={() => (folderAction = undefined)}>Cancel</button
        ><button
          class:destructive={folderAction.kind === "delete"}
          class:primary={folderAction.kind !== "delete"}
          disabled={actionBusy}
          >{actionBusy
            ? "Working…"
            : folderAction.kind === "delete"
              ? "Delete folder and contents"
              : folderAction.kind === "create"
                ? "Create folder"
                : "Save folder"}</button
        >
      </div>
    </form>
  </dialog>{/if}
{#if noteMoveOpen}<dialog
    class="modal"
    use:modal
    aria-labelledby="move-note-title"
    oncancel={(e) => {
      e.preventDefault();
      if (!actionBusy) noteMoveOpen = false;
    }}
  >
    <form onsubmit={moveNote}>
      <h2 id="move-note-title">Move note</h2>
      <label
        >Destination folder<select
          aria-label="Destination folder"
          bind:value={destination}
          ><option value="">Top level</option
          >{#each folderRows(folders.filter((f) => f.access === "owner")) as { folder }}<option
              value={folder.id}>{folderPath(folders, folder.id)}</option
            >{/each}</select
        ></label
      >
      {#if actionError}<p role="alert" class="notice danger">
          {actionError}
        </p>{/if}
      <div class="actions">
        <button
          type="button"
          disabled={actionBusy}
          onclick={() => (noteMoveOpen = false)}>Close</button
        ><button class="primary" disabled={actionBusy}
          >{actionBusy ? "Moving…" : "Move note"}</button
        >
      </div>
    </form>
  </dialog>{/if}
{#if profileOpen}<Profile onclose={() => (profileOpen = false)} />{/if}
{#if shareTarget}<ShareDialog
    kind={shareTarget.kind}
    id={shareTarget.id}
    title={shareTarget.title}
    onclose={() => (shareTarget = undefined)}
    onchanged={sharedChanged}
  />{/if}
{#if deleteOpen}<dialog
    class="modal"
    use:modal
    oncancel={(e) => {
      e.preventDefault();
      if (!deleting) deleteOpen = false;
    }}
    aria-labelledby="delete-title"
  >
    <h2 id="delete-title">Delete this note?</h2>
    <p>
      “{draft?.title || "Untitled note"}” will be permanently deleted. This
      cannot be undone.
    </p>
    {#if deleteError}<p role="alert" class="notice danger">
        {deleteError}
      </p>{/if}
    <div class="actions">
      <button disabled={deleting} onclick={() => (deleteOpen = false)}
        >Keep note</button
      ><button class="destructive" disabled={deleting} onclick={deleteNote}
        >{deleting ? "Deleting…" : "Delete note"}</button
      >
    </div>
  </dialog>{/if}
{#if leaveTarget}<dialog
    class="modal"
    use:modal
    oncancel={(e) => {
      e.preventDefault();
      leaveTarget = "";
    }}
    aria-labelledby="leave-title"
  >
    <h2 id="leave-title">Your changes haven’t saved</h2>
    <p>
      Stay here to retry or resolve the conflict. Leaving will discard your
      unsaved draft.
    </p>
    <div class="actions">
      <button class="primary" onclick={() => (leaveTarget = "")}
        >Stay here</button
      ><button
        onclick={async () => {
          if (leaveTarget === "logout") {
            draft?.dispose();
            draft = undefined;
            leaveTarget = "";
            await logout();
          } else await discard();
        }}>Discard & leave</button
      >
    </div>
  </dialog>{/if}
{#if session.expired}<dialog
    class="reauth-card"
    use:modal
    oncancel={(e) => e.preventDefault()}
    aria-label="Sign in to continue"
  >
    <AuthForm
      reauth
      oncomplete={() => {
        if (draft) void draft.resume();
        void loadFolders();
        void search();
      }}
    />
  </dialog>{/if}

{#if passwordNotice}<p class="password-notice notice" role="status">
    {passwordNotice}
  </p>{/if}
{#if passwordOpen}<PasswordChange
    onclose={() => (passwordOpen = false)}
    oncomplete={() => {
      passwordOpen = false;
      passwordNotice = "Password updated.";
    }}
  />{/if}

<style>
  .password-notice {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    z-index: 40;
    max-width: calc(100vw - 2rem);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
</style>
