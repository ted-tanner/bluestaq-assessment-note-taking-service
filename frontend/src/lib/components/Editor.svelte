<script lang="ts">
  import { Editor } from "@tiptap/core";
  import StarterKit from "@tiptap/starter-kit";
  import { Markdown } from "@tiptap/markdown";
  import { TableKit } from "@tiptap/extension-table";
  import { TaskList, TaskItem } from "@tiptap/extension-list";
  import {
    Bold,
    Italic,
    Strikethrough,
    List,
    ListOrdered,
    ListTodo,
    Quote,
    Code,
    SquareCode,
    Minus,
    Link,
    Table,
    Undo2,
    Redo2,
    Heading1,
    Heading2,
    Heading3,
  } from "@lucide/svelte";
  import DOMPurify from "dompurify";
  import { modal } from "#lib/modal.js";
  import { untrack } from "svelte";
  import { importMarkdown, unsupportedMarkdown } from "#lib/editor-content.js";
  import { renderMarkdown, safeLink } from "#lib/markdown.js";
  let { body, onchange }: { body: string; onchange: (value: string) => void } =
    $props();
  // Source with unsupported constructs remains editable without conversion.
  // svelte-ignore state_referenced_locally
  let source = $state(unsupportedMarkdown(body));
  let editor = $state.raw<Editor>();
  let revision = $state(0);
  const active = $derived.by(() => {
    revision;
    return Object.fromEntries(
      tools.map((t) => [t.label, editor?.isActive(t.active, t.attrs) ?? false]),
    );
  });
  const inTable = $derived.by(() => {
    revision;
    return editor?.isActive("table") ?? false;
  });
  let linkOpen = $state(false);
  let link = $state("");
  let linkError = $state("");
  let conversionWarning = $state(false);
  const tools = [
    {
      label: "Heading 1",
      icon: Heading1,
      active: "heading",
      attrs: { level: 1 },
      run: (e: Editor) => e.chain().focus().toggleHeading({ level: 1 }).run(),
    },
    {
      label: "Heading 2",
      icon: Heading2,
      active: "heading",
      attrs: { level: 2 },
      run: (e: Editor) => e.chain().focus().toggleHeading({ level: 2 }).run(),
    },
    {
      label: "Heading 3",
      icon: Heading3,
      active: "heading",
      attrs: { level: 3 },
      run: (e: Editor) => e.chain().focus().toggleHeading({ level: 3 }).run(),
    },
    {
      label: "Bold",
      icon: Bold,
      active: "bold",
      run: (e: Editor) => e.chain().focus().toggleBold().run(),
    },
    {
      label: "Italic",
      icon: Italic,
      active: "italic",
      run: (e: Editor) => e.chain().focus().toggleItalic().run(),
    },
    {
      label: "Strikethrough",
      icon: Strikethrough,
      active: "strike",
      run: (e: Editor) => e.chain().focus().toggleStrike().run(),
    },
    {
      label: "Bulleted list",
      icon: List,
      active: "bulletList",
      run: (e: Editor) => e.chain().focus().toggleBulletList().run(),
    },
    {
      label: "Numbered list",
      icon: ListOrdered,
      active: "orderedList",
      run: (e: Editor) => e.chain().focus().toggleOrderedList().run(),
    },
    {
      label: "Task list",
      icon: ListTodo,
      active: "taskList",
      run: (e: Editor) => e.chain().focus().toggleTaskList().run(),
    },
    {
      label: "Quote",
      icon: Quote,
      active: "blockquote",
      run: (e: Editor) => e.chain().focus().toggleBlockquote().run(),
    },
    {
      label: "Inline code",
      icon: Code,
      active: "code",
      run: (e: Editor) => e.chain().focus().toggleCode().run(),
    },
    {
      label: "Code block",
      icon: SquareCode,
      active: "codeBlock",
      run: (e: Editor) => e.chain().focus().toggleCodeBlock().run(),
    },
    {
      label: "Horizontal rule",
      icon: Minus,
      active: "horizontalRule",
      run: (e: Editor) => e.chain().focus().setHorizontalRule().run(),
    },
    {
      label: "Insert table",
      icon: Table,
      active: "table",
      run: (e: Editor) =>
        e
          .chain()
          .focus()
          .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
          .run(),
    },
  ];
  function mount(element: HTMLDivElement) {
    let dead = false;
    const instance = new Editor({
      element,
      extensions: [
        StarterKit.configure({
          underline: false,
          heading: { levels: [1, 2, 3, 4, 5, 6] },
          link: {
            openOnClick: false,
            autolink: false,
            isAllowedUri: safeLink,
            HTMLAttributes: { rel: "noopener noreferrer", target: "_blank" },
          },
        }),
        Markdown,
        TableKit,
        TaskList,
        TaskItem.configure({
          nested: true,
          HTMLAttributes: { "data-type": "taskItem" },
        }),
      ],
      // Use the same restricted import path as the reader. Source remains authoritative until an edit.
      content: "",
      editorProps: {
        attributes: {
          role: "textbox",
          "aria-label": "Note body",
          "aria-multiline": "true",
          class: "prose rich-input",
        },
        transformPastedHTML: (html) =>
          DOMPurify.sanitize(html, {
            FORBID_TAGS: ["img", "video", "audio", "iframe", "style", "script"],
            FORBID_ATTR: ["style"],
          }),
      },
      onTransaction: () => {
        if (!dead) untrack(() => revision++);
      },
      onUpdate: ({ editor: current }) => {
        if (!dead) untrack(() => onchange(current.getMarkdown()));
      },
    });
    instance.commands.setContent(importMarkdown(instance, body), {
      emitUpdate: false,
    });
    editor = instance;
    return {
      destroy() {
        dead = true;
        instance.destroy();
        editor = undefined;
      },
    };
  }
  function toRich() {
    // Compare a roundtrip before allowing conversion; unsupported source must never disappear silently.
    const probe = new Editor({
      extensions: [StarterKit, Markdown, TableKit, TaskList, TaskItem],
      content: renderMarkdown(body),
    });
    probe.commands.setContent(importMarkdown(probe, body), {
      emitUpdate: false,
    });
    const roundtrip = probe.getMarkdown();
    probe.destroy();
    if (roundtrip.trim() !== body.trim()) conversionWarning = true;
    else source = false;
  }
  function applyLink(event: SubmitEvent) {
    event.preventDefault();
    if (link && !safeLink(link)) {
      linkError = "Use an https://, http://, or mailto: link.";
      return;
    }
    if (link)
      editor
        ?.chain()
        .focus()
        .extendMarkRange("link")
        .setLink({ href: link })
        .run();
    else editor?.chain().focus().extendMarkRange("link").unsetLink().run();
    linkOpen = false;
  }
</script>

<div class="editor">
  <div class="editor-mode">
    <span class="muted"
      >{source ? "MARKDOWN SOURCE" : "MAKE ROOM FOR YOUR IDEAS"}</span
    >
    <div class="segmented" aria-label="Editor mode">
      <button
        class:active={!source}
        onclick={() => {
          if (source) toRich();
        }}>Rich text</button
      ><button class:active={source} onclick={() => (source = true)}
        >Markdown</button
      >
    </div>
  </div>
  {#if source}<textarea
      class="markdown-source"
      aria-label="Markdown source"
      value={body}
      oninput={(e) => onchange(e.currentTarget.value)}
      spellcheck="false"></textarea>
  {:else}
    <div class="format-toolbar" role="group" aria-label="Formatting">
      {#each tools as tool}<button
          class="icon-button"
          aria-label={tool.label}
          title={tool.label}
          aria-pressed={active[tool.label]}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => {
            if (editor) tool.run(editor);
          }}><tool.icon size={17} /></button
        >{/each}
      <button
        class="icon-button"
        aria-label="Insert link"
        title="Insert link"
        onclick={() => {
          link = editor?.getAttributes("link").href ?? "";
          linkError = "";
          linkOpen = true;
        }}><Link size={17} /></button
      >
      <span class="toolbar-divider"></span><button
        class="icon-button"
        aria-label="Undo"
        title="Undo"
        onclick={() => editor?.chain().focus().undo().run()}
        ><Undo2 size={17} /></button
      ><button
        class="icon-button"
        aria-label="Redo"
        title="Redo"
        onclick={() => editor?.chain().focus().redo().run()}
        ><Redo2 size={17} /></button
      >
    </div>
    {#if inTable}<div class="table-tools">
        <button
          class="text-button"
          onclick={() => editor?.chain().focus().addRowAfter().run()}
          >Add row</button
        ><button
          class="text-button"
          onclick={() => editor?.chain().focus().addColumnAfter().run()}
          >Add column</button
        ><button
          class="text-button"
          onclick={() => editor?.chain().focus().deleteRow().run()}
          >Delete row</button
        ><button
          class="text-button"
          onclick={() => editor?.chain().focus().deleteColumn().run()}
          >Delete column</button
        ><button
          class="text-button"
          onclick={() => editor?.chain().focus().deleteTable().run()}
          >Remove table</button
        >
      </div>{/if}
    <div use:mount></div>
  {/if}
</div>
{#if linkOpen}<dialog
    class="modal"
    use:modal
    oncancel={(e) => {
      e.preventDefault();
      linkOpen = false;
    }}
    aria-labelledby="link-title"
  >
    <h2 id="link-title">Add a link</h2>
    <form onsubmit={applyLink}>
      <label for="link-url">Link address</label><input
        id="link-url"
        type="text"
        bind:value={link}
        placeholder="https://example.com"
      />{#if linkError}<p role="alert">{linkError}</p>{/if}
      <p class="field-help">Leave empty to remove the link.</p>
      <div class="actions">
        <button type="button" onclick={() => (linkOpen = false)}>Cancel</button
        ><button class="primary">Apply link</button>
      </div>
    </form>
  </dialog>{/if}
{#if conversionWarning}<dialog
    class="modal"
    use:modal
    oncancel={(e) => {
      e.preventDefault();
      conversionWarning = false;
    }}
    aria-labelledby="conversion-title"
  >
    <h2 id="conversion-title">Convert to rich text?</h2>
    <p>
      Rich text may normalize Markdown formatting or remove unsupported syntax,
      including HTML and images. Your source stays unchanged until you edit in
      rich text.
    </p>
    <div class="actions">
      <button onclick={() => (conversionWarning = false)}>Keep source</button
      ><button
        class="primary"
        onclick={() => {
          conversionWarning = false;
          source = false;
        }}>Continue</button
      >
    </div>
  </dialog>{/if}
