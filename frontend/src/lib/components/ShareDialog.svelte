<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    ApiError,
    message,
    type User,
    type ShareEntry,
  } from "#lib/api.js";
  import { session } from "#lib/session.svelte.js";
  import { modal } from "#lib/modal.js";
  let {
    kind,
    id,
    title,
    onclose,
    onchanged,
  }: {
    kind: "notes" | "folders";
    id: string;
    title: string;
    onclose: () => void;
    onchanged: () => void;
  } = $props();
  let shares = $state<ShareEntry[]>([]),
    users = $state<User[]>([]);
  let query = $state(""),
    permission = $state<"view" | "edit">("view");
  let error = $state(""),
    searchError = $state(""),
    notice = $state("");
  let loading = $state(true),
    searching = $state(false),
    busy = $state(false);
  let controller: AbortController | undefined;
  let generation = 0;
  const path = $derived(`/${kind}/${id}/shares`);
  function failed(e: unknown) {
    error = message(e);
    if (e instanceof ApiError && e.status === 401) session.expired = true;
  }
  async function load() {
    loading = true;
    error = "";
    try {
      shares = await api<ShareEntry[]>(path);
    } catch (e) {
      failed(e);
    } finally {
      loading = false;
    }
  }
  onMount(() => {
    void load();
    return () => {
      generation++;
      controller?.abort();
    };
  });
  $effect(() => {
    const text = query.trim();
    const current = ++generation;
    controller?.abort();
    users = [];
    searchError = "";
    if ([...text].length < 2) {
      searching = false;
      return;
    }
    searching = true;
    const timer = setTimeout(async () => {
      controller = new AbortController();
      try {
        const result = await api<User[]>(
          `/users/search?${new URLSearchParams({ q: text })}`,
          { signal: controller.signal },
        );
        if (current === generation) users = result;
      } catch (e) {
        if (
          current === generation &&
          !(e instanceof DOMException && e.name === "AbortError")
        ) {
          searchError = message(e);
          if (e instanceof ApiError && e.status === 401) session.expired = true;
        }
      } finally {
        if (current === generation) searching = false;
      }
    }, 250);
    return () => {
      clearTimeout(timer);
      controller?.abort();
    };
  });
  async function change(user: User, value: "view" | "edit" | null) {
    if (busy) return;
    const adding =
      !!value && !shares.some((share) => share.user.id === user.id);
    busy = true;
    error = "";
    notice = "";
    try {
      await api(`${path}/${user.id}`, {
        method: value ? "PUT" : "DELETE",
        ...(value ? { body: JSON.stringify({ permission: value }) } : {}),
      });
      await load();
      if (adding) query = "";
      notice = value
        ? `Access updated for ${user.name || user.email}.`
        : `Direct access removed for ${user.name || user.email}.`;
      onchanged();
    } catch (e) {
      failed(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  class="modal share-dialog"
  use:modal
  aria-labelledby="share-title"
  oncancel={(e) => {
    e.preventDefault();
    if (!busy) onclose();
  }}
>
  <h2 id="share-title">Share {kind === "notes" ? "note" : "folder"}</h2>
  <p class="share-resource">{title}</p>
  <p>
    {kind === "folders"
      ? "Access includes all nested folders and notes, including future additions."
      : "Only this note is shared. Its containing folder stays private."}
  </p>
  <details class="sharing-help">
    <summary>How access works</summary>
    <p class="muted">
      Can edit allows editing notes and folder names, and adding notes or
      subfolders to editable shared folders. Only the owner can move, delete, or
      manage sharing.
    </p>
    <p class="muted">
      Access inherited from another shared folder remains until that folder’s
      owner removes it.
    </p>
  </details>
  <label
    >Find a person<input
      aria-label="Find a person by name or email"
      placeholder="Name or email (at least 2 characters)"
      bind:value={query}
      maxlength="100"
      autocomplete="off"
    /></label
  >
  <label
    >Access for new recipients<select
      aria-label="Access for new recipients"
      bind:value={permission}
      ><option value="view">Can view</option><option value="edit"
        >Can edit</option
      ></select
    ></label
  >
  {#if searching}<p role="status">Searching people…</p>{:else if searchError}<p
      role="alert"
    >
      {searchError}
    </p>{:else if query.trim().length >= 2 && !users.length}<p>
      No matching users.
    </p>{/if}
  <ul class="people-list" aria-label="Matching users">
    {#each users as user (user.id)}<li>
        <div>
          <strong>{user.name || "Unnamed user"}</strong><small
            >{user.email}</small
          >
        </div>
        <button
          disabled={busy ||
            loading ||
            shares.some((s) => s.user.id === user.id)}
          onclick={() => void change(user, permission)}
          aria-label={`Share with ${user.email}`}
          >{shares.some((s) => s.user.id === user.id)
            ? "Already shared"
            : "Share"}</button
        >
      </li>{/each}
  </ul>
  <h3>People with direct access</h3>
  {#if loading}<p role="status">Loading access…</p>{:else if !shares.length}<p>
      No direct shares yet.
    </p>{/if}
  <ul class="people-list" aria-label="People with access">
    {#each shares as share (share.user.id)}<li>
        <div>
          <strong>{share.user.name || "Unnamed user"}</strong><small
            >{share.user.email}</small
          >
        </div>
        <select
          aria-label={`Access for ${share.user.email}`}
          value={share.permission}
          disabled={busy}
          onchange={(e) =>
            void change(share.user, e.currentTarget.value as "view" | "edit")}
          ><option value="view">Can view</option><option value="edit"
            >Can edit</option
          ></select
        ><button
          class="quiet"
          aria-label={`Remove access for ${share.user.email}`}
          disabled={busy}
          onclick={() => void change(share.user, null)}>Remove</button
        >
      </li>{/each}
  </ul>
  {#if error}<p class="notice danger" role="alert">{error}</p>
    <button disabled={busy} onclick={() => void load()}>Reload access</button
    >{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  <div class="actions">
    <button class="primary" disabled={busy} onclick={onclose}>Done</button>
  </div>
</dialog>
