import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "@tiptap/markdown";
import { TableKit } from "@tiptap/extension-table";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { importMarkdown, unsupportedMarkdown } from "./editor-content";
describe("Markdown editor import", () => {
  it("roundtrips task lists, tables, links, and code using the official Markdown extension", () => {
    const editor = new Editor({
      extensions: [StarterKit, Markdown, TableKit, TaskList, TaskItem],
    });
    const source =
      "# Heading\n\n- [x] Done\n- [ ] Todo\n\n| A | B |\n| --- | --- |\n| One | Two |\n\n[link](https://example.com)\n\n```html\n<script>literal</script>\n```";
    editor.commands.setContent(importMarkdown(editor, source), {
      emitUpdate: false,
    });
    const result = editor.getMarkdown();
    expect(result).toContain("[x] Done");
    expect(result).toContain("[ ] Todo");
    expect(result).toContain("https://example.com");
    expect(result).toContain("<script>literal</script>");
    expect(editor.getJSON().content?.some((n) => n.type === "table")).toBe(
      true,
    );
    editor.destroy();
  });
  it("never parses source HTML into elements and strips unsafe link marks", () => {
    const editor = new Editor({ extensions: [StarterKit, Markdown] });
    const content = importMarkdown(
      editor,
      '<img src="https://tracker.test/pixel">\n\n[bad](javascript:alert%281%29)',
    );
    editor.commands.setContent(content, { emitUpdate: false });
    expect(editor.getHTML()).not.toContain("<img");
    expect(editor.getHTML()).not.toContain('href="javascript:');
    expect(editor.getText()).toContain("<img");
    editor.destroy();
  });
  it("recognizes unsupported constructs that must start in source mode", () => {
    expect(unsupportedMarkdown("![image](https://example.com)")).toBe(true);
    expect(unsupportedMarkdown("<div>html</div>")).toBe(true);
    expect(unsupportedMarkdown("# Heading\n\n**bold**")).toBe(false);
  });
  it("retains safe line breaks inside tables without enabling other HTML", () => {
    const editor = new Editor({ extensions: [StarterKit, Markdown, TableKit] });
    editor.commands.setContent(
      importMarkdown(
        editor,
        "| A | B |\n| --- | --- |\n| one<br>two | other |",
      ),
      { emitUpdate: false },
    );
    expect(editor.getHTML()).toContain("one<br>two");
    expect(editor.getMarkdown()).toContain("one<br>two");
    editor.destroy();
  });
  it("does not mistake code examples for unsupported HTML or images", () => {
    expect(
      unsupportedMarkdown(
        "```html\n<div>example</div>\n![image](test.png)\n```",
      ),
    ).toBe(false);
    expect(unsupportedMarkdown("Use `<Widget>` or `![image](url)` here.")).toBe(
      false,
    );
    expect(unsupportedMarkdown("    <div>indented code</div>")).toBe(false);
    expect(
      unsupportedMarkdown(
        "![image][photo]\n\n[photo]: https://example.com/photo.png",
      ),
    ).toBe(true);
  });
});
