<script lang="ts">
  import type { Snippet } from "svelte";
  import { Ellipsis } from "@lucide/svelte";

  let { label, children }: { label: string; children: Snippet } = $props();
  const id = $props.id();
  let menu: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let open = $state(false);

  function dismiss(event: MouseEvent) {
    if (!open || !(event.target instanceof Node)) return;
    if (!menu.contains(event.target)) open = false;
    else if (
      event.target instanceof Element &&
      event.target.closest(".action-menu-items button")
    ) {
      open = false;
    }
  }
</script>

<svelte:window
  onclick={dismiss}
  onkeydown={(event) => {
    if (event.key === "Escape" && open) {
      open = false;
      trigger.focus();
    }
  }}
/>

<div class="action-menu" bind:this={menu}>
  <button
    class="icon-button"
    aria-label={label}
    aria-expanded={open}
    aria-controls={id}
    title={label}
    bind:this={trigger}
    onclick={() => (open = !open)}
  >
    <Ellipsis size={18} />
  </button>
  <div class="action-menu-items" {id} hidden={!open}>{@render children()}</div>
</div>
