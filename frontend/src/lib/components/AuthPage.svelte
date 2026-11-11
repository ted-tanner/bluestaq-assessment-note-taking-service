<script lang="ts">
  import AuthForm from "./AuthForm.svelte";
  import { NotebookPen, Sparkles } from "@lucide/svelte";
  let { mode }: { mode: "signin" | "register" | "reset" } = $props();
</script>

<svelte:head
  ><title
    >{mode === "register"
      ? "Create account"
      : mode === "reset"
        ? "Recover account"
        : "Sign in"} · Bluestaq Notes</title
  ></svelte:head
>
<div class="auth-page">
  <aside class="auth-story">
    <a class="brand" href="/"
      ><span class="brand-mark"><NotebookPen size={22} /></span> bluestaq<span
        class="brand-light">notes</span
      ></a
    >
    <div class="story-content">
      <span class="story-tag"><Sparkles size={14} /> ROOM TO THINK</span>
      <h2>Small notes.<br />Clearer thinking.</h2>
      <p>
        A quiet place to collect your thoughts,<br />work through ideas, and
        make them yours.
      </p>
      <div class="sample-note">
        <span class="sample-date">A NOTE TO SELF</span>
        <h3>Good ideas start somewhere.</h3>
        <p>Give them a little room.</p>
        <div class="sample-line"></div>
        <div class="sample-line short"></div>
        <span class="sample-check">✓ &nbsp; Keep the things that matter.</span>
      </div>
    </div>
  </aside>
  <main class="auth-main"><AuthForm {mode} /></main>
</div>
