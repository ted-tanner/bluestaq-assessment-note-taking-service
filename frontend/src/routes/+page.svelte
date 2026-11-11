<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { session } from "#lib/session.svelte.js";
  onMount(() => goto(session.user ? "/notes" : "/sign-in", { replace: true }));
</script>

<p class="boot-message" role="status">Opening your workspace…</p>
