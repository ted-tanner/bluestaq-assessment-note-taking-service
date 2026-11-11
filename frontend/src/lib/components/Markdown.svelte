<script lang="ts">
  import { renderMarkdown } from "#lib/markdown.js";
  let { body }: { body: string } = $props();
</script>

<div class="prose">{@html renderMarkdown(body)}</div>
