<script lang="ts">
  import { api, ApiError, message, type User } from "#lib/api.js";
  import { session } from "#lib/session.svelte.js";
  import { modal } from "#lib/modal.js";
  let { onclose }: { onclose: () => void } = $props();
  let name = $state(session.user?.name ?? "");
  let busy = $state(false),
    error = $state("");
  const required = $derived(!session.user?.name?.trim());
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = "";
    try {
      session.user = await api<User>("/user/me", {
        method: "PATCH",
        body: JSON.stringify({ name: name.trim() }),
      });
      onclose();
    } catch (e) {
      error = message(e);
      if (e instanceof ApiError && e.status === 401) session.expired = true;
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  class="modal"
  use:modal
  aria-labelledby="profile-title"
  oncancel={(event) => {
    event.preventDefault();
    if (!required && !busy) onclose();
  }}
>
  <form onsubmit={save}>
    <h2 id="profile-title">{required ? "Add your name" : "Your profile"}</h2>
    <p>Your name helps people find you when sharing notes and folders.</p>
    <label
      >Full name<input
        aria-label="Full name"
        autocomplete="name"
        required
        maxlength="100"
        bind:value={name}
        disabled={busy}
      /></label
    >
    <p class="muted">{session.user?.email}</p>
    {#if error}<p class="notice danger" role="alert">{error}</p>{/if}
    <div class="actions">
      {#if !required}<button type="button" disabled={busy} onclick={onclose}
          >Cancel</button
        >{/if}<button class="primary" disabled={busy || !name.trim()}
        >{busy ? "Saving…" : "Save profile"}</button
      >
    </div>
  </form>
</dialog>
