<script lang="ts">
  import { untrack } from "svelte";
  import type { Folder, Note } from "#lib/api.js";
  import { folderRows } from "#lib/folders.js";
  import { highlightedText, noteExcerpt } from "#lib/markdown.js";
  import {
    Folder as FolderIcon,
    FileText,
    ChevronRight,
    ChevronDown,
    Plus,
  } from "@lucide/svelte";
  let {
    folders,
    notes,
    query,
    selected,
    location,
    href,
    noteHref,
    oncreate,
    error,
    onretry,
    canCreate = true,
  }: {
    folders: Folder[];
    notes: Note[];
    query: string;
    selected: string;
    location: string;
    href: (id: string) => string;
    noteHref: (note: Note) => string;
    oncreate: () => void;
    error: string;
    onretry: () => void;
    canCreate?: boolean;
  } = $props();
  let expanded = $state(new Set<string>());
  $effect(() => {
    let id: string | null | undefined = location;
    const parents = new Set<string>();
    while (id && !parents.has(id)) {
      parents.add(id);
      id = folders.find((f) => f.id === id)?.parent_id;
    }
    untrack(() => {
      if ([...parents].some((id) => !expanded.has(id)))
        expanded = new Set([...expanded, ...parents]);
    });
  });
  const filtering = $derived(!!query.trim());
  const matchingFolders = $derived.by(() => {
    const ids = new Set<string>();
    for (const note of notes) {
      let id = note.folder_id;
      while (id && !ids.has(id)) {
        ids.add(id);
        id = folders.find((folder) => folder.id === id)?.parent_id ?? null;
      }
    }
    return ids;
  });
  const visibleFolders = $derived(
    filtering
      ? folders.filter((folder) => matchingFolders.has(folder.id))
      : folders,
  );
  const rows = $derived(
    folderRows(
      visibleFolders.filter((f) => f.access === "owner"),
      filtering ? undefined : expanded,
    ),
  );
  const sharedRows = $derived(
    folderRows(
      visibleFolders.filter((f) => f.access !== "owner"),
      filtering ? undefined : expanded,
    ),
  );
</script>

{#snippet highlight(text: string)}
  {#each highlightedText(text, query) as part}{#if part.match}<strong
        >{part.text}</strong
      >{:else}{part.text}{/if}{/each}
{/snippet}

{#snippet noteLink(note: Note, depth: number)}
  <a
    class="tree-note"
    class:selected={selected === note.id}
    style:margin-left={`min(${depth * 16 + 28}px, 40%)`}
    href={noteHref(note)}
    aria-current={selected === note.id ? "page" : undefined}
  >
    <FileText size={14} />
    <span class="tree-note-copy">
      <span class="tree-note-title"
        >{@render highlight(note.title || "Untitled note")}</span
      >
      <span class="tree-note-preview"
        >{@render highlight(noteExcerpt(note.body, query))}</span
      >
      {#if note.access !== "owner"}<small class="shared-by"
          >Shared by {note.owner.name || note.owner.email} · {note.access ===
          "edit"
            ? "Can edit"
            : "Can view"}</small
        >{/if}
    </span>
  </a>
{/snippet}

<nav class="folder-navigation" id="folder-sidebar" aria-label="Folders">
  <div class="folder-heading">
    <strong>Folders & notes</strong>
    <button
      class="icon-button"
      aria-label="New folder"
      disabled={!canCreate}
      onclick={oncreate}><Plus size={16} /></button
    >
  </div>
  <a
    href={href("")}
    class:active={location === ""}
    aria-current={location === "" ? "page" : undefined}>All notes</a
  >
  <a
    href={href("shared")}
    class:active={location === "shared"}
    aria-current={location === "shared" ? "page" : undefined}>Shared with me</a
  >
  {#if error}<p role="alert">{error}</p>
    <button onclick={onretry}>Retry folders</button>{/if}
  <div class="folder-tree">
    {#each rows as { folder, depth, hasChildren } (folder.id)}
      {@const open = filtering || expanded.has(folder.id)}
      {@const folderNotes = notes.filter(
        (note) => note.folder_id === folder.id,
      )}
      <div
        class="folder-tree-row"
        style:padding-left={`min(${depth * 16}px, 40%)`}
      >
        {#if hasChildren || folderNotes.length}
          <button
            class="icon-button"
            aria-label={`${open ? "Collapse" : "Expand"} ${folder.name}`}
            aria-expanded={open}
            disabled={filtering}
            onclick={() => {
              const next = new Set(expanded);
              if (next.has(folder.id)) next.delete(folder.id);
              else next.add(folder.id);
              expanded = next;
            }}
          >
            {#if open}<ChevronDown size={14} />{:else}<ChevronRight
                size={14}
              />{/if}
          </button>
        {:else}<span class="tree-spacer"></span>{/if}
        <a
          href={href(folder.id)}
          title={folder.name}
          class:active={location === folder.id}
          aria-current={location === folder.id ? "page" : undefined}
        >
          <FolderIcon size={15} /><span>{folder.name}</span>
        </a>
      </div>
      {#if open}{#each folderNotes as note (note.id)}{@render noteLink(
            note,
            depth + 1,
          )}{/each}{/if}
    {/each}
    {#if sharedRows.length}<p class="shared-tree-label">Shared folders</p>{/if}
    {#each sharedRows as { folder, depth, hasChildren } (folder.id)}
      {@const open = filtering || expanded.has(folder.id)}
      {@const folderNotes = notes.filter(
        (note) => note.folder_id === folder.id,
      )}
      <div
        class="folder-tree-row"
        style:padding-left={`min(${depth * 16}px, 40%)`}
      >
        {#if hasChildren || folderNotes.length}
          <button
            class="icon-button"
            aria-label={`${open ? "Collapse" : "Expand"} ${folder.name}`}
            aria-expanded={open}
            disabled={filtering}
            onclick={() => {
              const next = new Set(expanded);
              if (next.has(folder.id)) next.delete(folder.id);
              else next.add(folder.id);
              expanded = next;
            }}
          >
            {#if open}<ChevronDown size={14} />{:else}<ChevronRight
                size={14}
              />{/if}
          </button>
        {:else}<span class="tree-spacer"></span>{/if}
        <a
          href={href(folder.id)}
          title={folder.name}
          class:active={location === folder.id}
          aria-current={location === folder.id ? "page" : undefined}
        >
          <FolderIcon size={15} /><span>{folder.name}</span>
        </a>
      </div>
      {#if open}{#each folderNotes as note (note.id)}{@render noteLink(
            note,
            depth + 1,
          )}{/each}{/if}
    {/each}
    {#each notes.filter((note) => !note.folder_id || !folders.some((folder) => folder.id === note.folder_id)) as note (note.id)}{@render noteLink(
        note,
        0,
      )}{/each}
  </div>
</nav>
