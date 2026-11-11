import type { Folder } from "./api";

export function descendants(folders: Folder[], id: string): Set<string> {
  const ids = new Set([id]);
  const children = new Map<string, string[]>();
  for (const folder of folders) {
    if (folder.parent_id)
      children.set(folder.parent_id, [
        ...(children.get(folder.parent_id) ?? []),
        folder.id,
      ]);
  }
  const queue = [id];
  for (let i = 0; i < queue.length; i++) {
    for (const child of children.get(queue[i]) ?? []) {
      if (!ids.has(child)) {
        ids.add(child);
        queue.push(child);
      }
    }
  }
  return ids;
}
export function folderPath(folders: Folder[], id: string | null): string {
  const names: string[] = [];
  const seen = new Set<string>();
  const byId = new Map(folders.map((f) => [f.id, f]));
  while (id && !seen.has(id)) {
    seen.add(id);
    const folder = byId.get(id);
    if (!folder) break;
    names.push(folder.name);
    id = folder.parent_id;
  }
  return names.reverse().join(" / ") || "Top level";
}
export function folderRows(folders: Folder[], expanded?: Set<string>) {
  const rows: { folder: Folder; depth: number; hasChildren: boolean }[] = [];
  const children = new Map<string | null, Folder[]>();
  for (const folder of folders)
    children.set(folder.parent_id, [
      ...(children.get(folder.parent_id) ?? []),
      folder,
    ]);
  for (const list of children.values())
    list.sort(
      (a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id),
    );
  const stack = (children.get(null) ?? [])
    .map((folder) => ({ folder, depth: 0 }))
    .reverse();
  const seen = new Set<string>();
  while (stack.length) {
    const row = stack.pop()!;
    if (seen.has(row.folder.id)) continue;
    seen.add(row.folder.id);
    const nested = children.get(row.folder.id) ?? [];
    rows.push({ ...row, hasChildren: !!nested.length });
    if (!expanded || expanded.has(row.folder.id)) {
      for (let i = nested.length - 1; i >= 0; i--)
        stack.push({ folder: nested[i], depth: row.depth + 1 });
    }
  }
  return rows;
}
export const viewKey = (owner: string) => `bluestaq-note-view:${owner}`;
export function storedView(value: string | null): "tile" | "list" {
  return value === "list" ? "list" : "tile";
}
