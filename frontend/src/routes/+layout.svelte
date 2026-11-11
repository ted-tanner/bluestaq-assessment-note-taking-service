<script lang="ts">
  import "@fontsource-variable/inter";
  import "./app.css";
  import favicon from "#lib/assets/favicon.svg";
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { session, initialize } from "#lib/session.svelte.js";
  import type { LayoutProps } from "./$types";
  let { children }: LayoutProps = $props();
  onMount(() => {
    try {
      const theme = localStorage.getItem("bluestaq-theme");
      if (theme === "light" || theme === "dark")
        document.documentElement.dataset.theme = theme;
    } catch {
      /* Storage may be disabled; the system theme still works. */
    }
    void initialize();
  });
  $effect(() => {
    if (!session.ready || session.error) return;
    const auth = ["/sign-in", "/register", "/recover"].includes(
      page.url.pathname,
    );
    if (session.user && auth) void goto("/notes", { replace: true });
    else if (!session.user && page.url.pathname.startsWith("/notes"))
      void goto("/sign-in", { replace: true });
  });
</script>

<svelte:head
  ><link rel="icon" href={favicon} /><meta
    name="description"
    content="A quiet, private workspace for your notes and ideas."
  /></svelte:head
>
{#if !session.ready}<div class="boot-message" role="status">
    Opening your workspace…
  </div>
{:else if session.error}<main class="boot-message">
    <p role="alert">{session.error}</p>
    <button class="primary" onclick={initialize}>Retry connection</button>
  </main>
{:else}{@render children()}{/if}
