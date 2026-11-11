import { describe, expect, it } from "vitest";
import { renderMarkdown, noteExcerpt, highlightedText } from "./markdown";
describe("private Markdown rendering", () => {
  it("renders common Markdown and tables", () => {
    const result = renderMarkdown(
      "# Hello\n\n**bold** and `code`\n\n| A | B |\n| - | - |\n| 1 | 2 |",
    );
    expect(result).toContain("<h1>Hello</h1>");
    expect(result).toContain("<strong>bold</strong>");
    expect(result).toContain("<table>");
  });
  it("blocks raw HTML, scripts, tracking images, and unsafe links", () => {
    const result = renderMarkdown(
      '<script>alert(1)</script>\n\n<img src="https://tracker.test/x" onerror="alert(2)">\n\n![secret](https://tracker.test/pixel)\n\n[bad](javascript:alert%281%29)\n\n[good](https://example.com)',
    );
    expect(result).not.toContain("<script");
    expect(result).not.toContain("<img");
    expect(result).not.toContain("javascript:");
    expect(result).not.toContain("tracker.test");
    expect(result).toContain('rel="noopener noreferrer"');
  });
  it("keeps code literal and never enables embedded controls", () => {
    const result = renderMarkdown(
      "```html\n<script>bad()</script>\n```\n\n- [x] done",
    );
    expect(result).toContain("&lt;script&gt;");
    const container = document.createElement("div");
    container.innerHTML = result;
    expect(container.querySelector("input")?.disabled).toBe(true);
  });
  it("allows only bare line-break HTML needed by Markdown table cells", () => {
    const html = renderMarkdown(
      '| A |\n| --- |\n| one<br>two |\n\n<br onclick="alert(1)">',
    );
    expect(html).toContain("one<br>two");
    expect(html).not.toContain("onclick");
  });
});

describe("note previews", () => {
  it("uses readable link labels and checklist text rather than Markdown syntax", () => {
    expect(
      noteExcerpt(
        "# Plan\n\nRead [the project](https://example.com).\n\n- [x] Done\n- [ ] Next",
      ),
    ).toBe("Plan Read the project. Done Next");
    expect(noteExcerpt("<script>bad()</script>")).toBe(
      "A blank page, full of possibility.",
    );
    expect(noteExcerpt("")).toBe("A blank page, full of possibility.");
  });
});

describe("search previews", () => {
  it("shows matching content beyond the beginning of a note as plain text", () => {
    const preview = noteExcerpt(
      "Opening context. ".repeat(30) +
        "**Astronomy** findings and [sources](https://example.com)",
      "astronomy",
    );
    expect(preview).toContain("Astronomy findings and sources");
    expect(preview).toMatch(/^…/);
    expect(preview).not.toContain("**");
    expect(preview).not.toContain("https://");
  });
  it("highlights literal, case-insensitive matches without interpreting HTML", () => {
    expect(highlightedText("A [test] and [TEST] <script>", "[test]")).toEqual([
      { text: "A ", match: false },
      { text: "[test]", match: true },
      { text: " and ", match: false },
      { text: "[TEST]", match: true },
      { text: " <script>", match: false },
    ]);
    expect(highlightedText("Plain text", "  ")).toEqual([
      { text: "Plain text", match: false },
    ]);
  });
});
