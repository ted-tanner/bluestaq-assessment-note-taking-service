import { expect, it } from "vitest";
import type { Folder } from "./api";
import {
  descendants,
  folderRows,
  folderPath,
  storedView,
  viewKey,
} from "./folders";
const folder = (id: string, parent_id: string | null = null): Folder => ({
  id,
  access: "owner",
  owner: { id: "owner", name: "Owner", email: "owner@test.test" },
  name: id,
  parent_id,
  revision: 1,
  created_at: 1,
  updated_at: 1,
});
it("traverses arbitrary folder depth, keeps siblings out, and respects expansion", () => {
  const folders = [
    folder("A"),
    folder("B", "A"),
    folder("C", "B"),
    folder("Other"),
  ];
  expect([...descendants(folders, "A")]).toEqual(["A", "B", "C"]);
  expect(folderPath(folders, "C")).toBe("A / B / C");
  expect(folderPath(folders, null)).toBe("Top level");
  expect(
    folderRows(folders, new Set(["A"])).map((r) => [r.folder.id, r.depth]),
  ).toEqual([
    ["A", 0],
    ["B", 1],
    ["Other", 0],
  ]);
  expect(folderRows(folders).map((r) => r.folder.id)).toEqual([
    "A",
    "B",
    "C",
    "Other",
  ]);
});
it("uses tiles by default and isolates preferences between accounts", () => {
  expect(storedView(null)).toBe("tile");
  expect(storedView("invalid")).toBe("tile");
  expect(storedView("list")).toBe("list");
  expect(viewKey("alice")).not.toBe(viewKey("bob"));
});
