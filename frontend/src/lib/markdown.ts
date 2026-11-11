import { marked } from "marked";
import DOMPurify from "dompurify";
export function safeLink(url: string) {
  return /^(https?:\/\/|mailto:)/i.test(url);
}
export function renderMarkdown(source: string): string {
  const renderer = new marked.Renderer();
  renderer.html = ({ text }) =>
    /^<br\s*\/?>$/i.test(text.trim()) ? "<br>" : ""; // Raw HTML is not part of our document format.
  renderer.image = ({ text }) => text; // Never send a reader's IP or referrer to image hosts.
  renderer.link = ({ href, tokens }) => {
    const label = marked.Parser.parseInline(tokens, { renderer });
    if (!safeLink(href)) return label;
    const escaped = href
      .replaceAll("&", "&amp;")
      .replaceAll('"', "&quot;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;");
    return `<a href="${escaped}" target="_blank" rel="noopener noreferrer">${label}</a>`;
  };
  return DOMPurify.sanitize(
    marked.parse(source, { renderer, async: false, gfm: true }) as string,
    {
      ALLOWED_TAGS: [
        "p",
        "br",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "strong",
        "em",
        "s",
        "del",
        "ul",
        "ol",
        "li",
        "blockquote",
        "pre",
        "code",
        "hr",
        "a",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "input",
      ],
      ALLOWED_ATTR: [
        "href",
        "target",
        "rel",
        "start",
        "type",
        "checked",
        "disabled",
        "align",
      ],
      ALLOW_DATA_ATTR: false,
    },
  );
}

export function noteExcerpt(source: string, query = ""): string {
  const parts: string[] = [];
  marked.walkTokens(marked.lexer(source), (token) => {
    if (token.type === "text" && !token.tokens?.length) parts.push(token.text);
    else if (
      token.type === "code" ||
      token.type === "codespan" ||
      token.type === "escape"
    )
      parts.push(token.text);
    else if (token.type === "image") parts.push(token.text);
  });
  const plain = parts
    .join(" ")
    .replace(/\s+/g, " ")
    .replace(/\s+([.,!?;:])/g, "$1")
    .trim();
  if (!plain) return "A blank page, full of possibility.";
  const match = query.trim()
    ? plain.toLowerCase().indexOf(query.trim().toLowerCase())
    : -1;
  const start = match > 45 ? match - 45 : 0;
  const end = Math.min(
    plain.length,
    Math.max(start + 130, match + query.trim().length),
  );
  return (
    (start ? "…" : "") +
    plain.slice(start, end) +
    (end < plain.length ? "…" : "")
  );
}

export function highlightedText(text: string, query: string) {
  const term = query.trim();
  if (!term) return [{ text, match: false }];
  const parts: { text: string; match: boolean }[] = [];
  const lower = text.toLowerCase();
  let start = 0;
  let index = lower.indexOf(term.toLowerCase());
  while (index !== -1) {
    if (index > start)
      parts.push({ text: text.slice(start, index), match: false });
    parts.push({ text: text.slice(index, index + term.length), match: true });
    start = index + term.length;
    index = lower.indexOf(term.toLowerCase(), start);
  }
  if (start < text.length)
    parts.push({ text: text.slice(start), match: false });
  return parts;
}
