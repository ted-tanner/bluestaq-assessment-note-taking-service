import { marked } from "marked";
import type { Editor, JSONContent } from "@tiptap/core";
import { safeLink } from "./markdown";
export function importMarkdown(editor: Editor, source: string): JSONContent {
  const manager = editor.markdown!;
  // Disable HTML token recognition before parsing, so no raw HTML reaches a DOM parser.
  const tokenizer = new manager.instance.Tokenizer();
  tokenizer.html = () => undefined;
  const parseTag = tokenizer.tag.bind(tokenizer);
  tokenizer.tag = (source) =>
    /^<br\s*\/?>/i.test(source) ? parseTag(source) : undefined;
  const previousTokenizer = manager.instance.defaults.tokenizer;
  let document: JSONContent;
  try {
    manager.instance.setOptions({ tokenizer });
    document = manager.parse(source);
  } finally {
    manager.instance.setOptions({ tokenizer: previousTokenizer });
  }
  function clean(node: JSONContent): JSONContent {
    return {
      ...node,
      marks: node.marks?.filter(
        (mark) =>
          mark.type !== "link" || safeLink(String(mark.attrs?.href ?? "")),
      ),
      content: node.content
        ?.filter((child) => child.type !== "image")
        .map(clean),
    };
  }
  return clean(document);
}
export function unsupportedMarkdown(source: string): boolean {
  let unsupported = false;
  marked.walkTokens(marked.lexer(source), (token) => {
    // Inspect parsed constructs, not source spelling: HTML inside code is ordinary text.
    if (
      token.type === "image" ||
      (token.type === "html" && !/^<br\s*\/?>$/i.test(token.raw.trim()))
    )
      unsupported = true;
    if (token.type === "text" && /\$\$|^\s*:::|\[\^[^\]]+\]/m.test(token.text))
      unsupported = true;
    if (token.type === "def" && token.tag.startsWith("^")) unsupported = true;
  });
  return unsupported;
}
