<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, json, message, type User } from "#lib/api.js";
  import { signedIn } from "#lib/session.svelte.js";
  import { modal } from "#lib/modal.js";

  let { onclose, oncomplete }: { onclose: () => void; oncomplete: () => void } =
    $props();
  let current = $state("");
  let password = $state("");
  let confirmation = $state("");
  let busy = $state(false);
  let error = $state("");
  function clear() {
    current = "";
    password = "";
    confirmation = "";
  }
  onDestroy(clear);
  function close() {
    if (!busy) {
      clear();
      onclose();
    }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    if (
      [...password].length < 12 ||
      new TextEncoder().encode(password).length > 1024
    ) {
      error =
        "Use at least 12 characters and no more than 1,024 bytes for your password.";
      return;
    }
    if (password !== confirmation) {
      error = "New passwords must match.";
      return;
    }
    busy = true;
    try {
      const result = await api<{ user: User }>("/auth/password-change", {
        method: "POST",
        body: json({ current_password: current, new_password: password }),
      });
      clear();
      signedIn(result.user);
      oncomplete();
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  class="modal"
  use:modal
  aria-labelledby="password-change-title"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <h2 id="password-change-title">Change password</h2>
  <p>
    You’ll stay signed in here. Other browsers will need to sign in again soon.
  </p>
  <form onsubmit={submit}>
    <label for="current-password">Current password</label>
    <input
      id="current-password"
      type="password"
      autocomplete="current-password"
      required
      bind:value={current}
      disabled={busy}
    />
    <label for="new-password">New password</label>
    <input
      id="new-password"
      type="password"
      autocomplete="new-password"
      required
      bind:value={password}
      disabled={busy}
      aria-describedby="new-password-help"
    />
    <p id="new-password-help" class="field-help">
      Use at least 12 characters
    </p>
    <label for="confirm-password">Confirm new password</label>
    <input
      id="confirm-password"
      type="password"
      autocomplete="new-password"
      required
      bind:value={confirmation}
      disabled={busy}
    />
    {#if error}<p class="notice danger" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="secondary" onclick={close} disabled={busy}
        >Cancel</button
      >
      <button type="submit" class="primary" disabled={busy}
        >{busy ? "Updating…" : "Update password"}</button
      >
    </div>
  </form>
</dialog>
