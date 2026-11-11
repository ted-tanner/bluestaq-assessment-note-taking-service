import { test, expect, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
const password = "A very long test password 123!";
const previousCodes = new Map<string, string>();
async function code(email: string, purpose = "registration") {
  let found = "";
  await expect
    .poll(() => {
      const path = readFileSync(
        process.env.E2E_INBOX_FILE || ".svelte-kit/e2e-inbox-path.txt",
        "utf8",
      );
      const mail = readFileSync(path, "utf8");
      const escaped = email.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const matches = [
        ...mail.matchAll(
          new RegExp(
            `To: ${escaped}\\nSubject: [^\\n]+\\nYour ${purpose} code is: ([A-Z0-9]{8})`,
            "g",
          ),
        ),
      ];
      found = matches.at(-1)?.[1] ?? "";
      return found === previousCodes.get(`${email}:${purpose}`) ? "" : found;
    })
    .toMatch(/^[A-Z0-9]{8}$/);
  previousCodes.set(`${email}:${purpose}`, found);
  return found;
}
async function register(page: Page, name = "Test Writer") {
  const email = `writer-${Date.now()}-${Math.random().toString(36).slice(2)}@example.test`;
  await page.goto("/register");
  await page.getByLabel("Full name", { exact: true }).fill(name);
  await page.getByLabel("Email address").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(page.getByLabel("Verification code")).toBeVisible();
  await expect(page.getByLabel("Verification code")).toBeFocused();
  await page.getByLabel("Verification code").fill(await code(email));
  await page.getByRole("button", { name: "Verify & continue" }).click();
  await expect(page).toHaveURL(/\/notes$/);
  return email;
}
async function newNote(page: Page, title: string, body = "A useful thought.") {
  await page.getByRole("link", { name: "New note", exact: true }).click();
  await page.getByLabel("Note title").fill(title);
  await page.getByRole("button", { name: "Markdown", exact: true }).click();
  await page.getByLabel("Markdown source").fill(body);
  await expect(page).toHaveURL(/\/notes\/[^/?]+\/edit(?:\?|$)/);
  await expect(page.locator(".save-status")).toContainText("All changes saved");
}
test("registration, Markdown autosave, reader, search, theme, and deletion", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  const email = await register(page);
  const cookies = await page.context().cookies();
  expect(cookies.find((c) => c.name === "access_token")?.httpOnly).toBe(true);
  expect(cookies.find((c) => c.name === "refresh_token")?.httpOnly).toBe(true);
  await newNote(
    page,
    "Project garden",
    "# A little clarity\n\n**Ideas** for tomorrow.\n\n- [x] Make space\n- [ ] Keep going\n\n| Thought | Next step |\n| --- | --- |\n| Write | Begin |",
  );
  await page.getByRole("link", { name: "Read", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Project garden", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".prose strong")).toHaveText("Ideas");
  await expect(page.locator(".prose table")).toBeVisible();
  await page.getByLabel("Search your notes").fill("garden");
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await page.getByLabel("Search your notes").fill("absentword");
  await expect(page.getByText("No notes found")).toBeVisible();
  await page.getByLabel("Clear search").click();
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/desktop-workspace.png`,
    fullPage: true,
  });
  await page.getByRole("button", { name: /Change theme/ }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.getByRole("button", { name: /Change theme/ }).click();
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/dark-workspace.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/mobile-workspace.png`,
    fullPage: true,
  });
  await expect(
    page.getByRole("link", { name: "All notes", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Delete note", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Delete note", exact: true })
    .click();
  await expect(page).toHaveURL(/\/notes$/);
  await expect(page.getByText("A fresh start")).toBeVisible();
  expect(await page.evaluate(() => Object.keys(localStorage))).toEqual([
    "bluestaq-theme",
  ]);
  expect(errors).toEqual([]);
  expect(email).toContain("@example.test");
});
test("sign-in requires 2FA and recovery requires a new sign-in", async ({
  page,
}) => {
  const email = await register(page);
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page.getByLabel("Email address").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByLabel("Verification code").fill("BADCODE1");
  await page.getByRole("button", { name: "Verify & continue" }).click();
  await expect(page.getByRole("alert")).toContainText(
    "That code is incorrect, expired, or already used",
  );
  await expect(page).toHaveURL(/sign-in/);
  await page.getByLabel("Verification code").fill(await code(email, "signin"));
  await page.getByRole("button", { name: "Verify & continue" }).click();
  await expect(page).toHaveURL(/\/notes$/);
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page.getByRole("link", { name: "Forgot password?" }).click();
  await expect(page).toHaveURL(/\/recover$/);
  await page.getByLabel("Email address").fill(email);
  await page.getByRole("button", { name: "Send recovery code" }).click();
  await page.getByLabel("Verification code").fill(await code(email, "reset"));
  const replacement = "Another very long password 456!";
  await page.getByLabel("New password", { exact: true }).fill(replacement);
  await page
    .getByRole("button", { name: "Reset password", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Password updated" }),
  ).toBeVisible();
  await page.getByRole("link", { name: /Back to sign in/ }).click();
  await expect(page).toHaveURL(/\/sign-in$/);
  await page.getByLabel("Email address").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(replacement);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByLabel("Verification code").fill(await code(email, "signin"));
  await page.getByRole("button", { name: "Verify & continue" }).click();
  await expect(page).toHaveURL(/\/notes$/);
});
test("two tabs detect conflicts and preserve the local draft", async ({
  page,
}) => {
  await register(page);
  await newNote(page, "Shared starting point", "Original body");
  const url = page.url();
  const second = await page.context().newPage();
  await second.goto(url);
  await expect(second.getByLabel("Note title")).toHaveValue(
    "Shared starting point",
  );
  await second.getByLabel("Note title").fill("Saved in the second tab");
  await expect(second.locator(".save-status")).toContainText(
    "All changes saved",
  );
  await page.getByLabel("Markdown source").fill("My unsaved local draft");
  await expect(
    page.getByRole("heading", { name: "This note changed elsewhere" }),
  ).toBeVisible();
  await expect(page.getByLabel("Markdown source")).toHaveValue(
    "My unsaved local draft",
  );
  await expect(
    page.getByText("Latest saved version: Saved in the second tab"),
  ).toBeVisible();
  await page.getByRole("button", { name: "Save draft as new note" }).click();
  await expect(page).not.toHaveURL(url);
  await expect(page.locator(".save-status")).toContainText("All changes saved");
  await second.close();
});
test("session expiry preserves the draft behind same-account reauthentication", async ({
  page,
}) => {
  const email = await register(page);
  await newNote(page, "A private draft");
  await page.context().clearCookies({ name: /^(access_token|refresh_token)$/ });
  await page.getByLabel("Markdown source").fill("Keep this while I sign in");
  await expect(
    page.getByRole("dialog", { name: "Sign in to continue" }),
  ).toBeVisible();
  const dialog = page.getByRole("dialog", { name: "Sign in to continue" });
  await dialog.getByLabel("Email address").fill(email);
  await dialog.getByLabel("Password", { exact: true }).fill(password);
  await dialog.getByRole("button", { name: "Sign in", exact: true }).click();
  await dialog
    .getByLabel("Verification code")
    .fill(await code(email, "signin"));
  await dialog.getByRole("button", { name: "Verify & continue" }).click();
  await expect(dialog).not.toBeVisible();
  await expect(page.getByLabel("Markdown source")).toHaveValue(
    "Keep this while I sign in",
  );
  await expect(page.locator(".save-status")).toContainText("All changes saved");
});
test("source conversion is explicit and Markdown cannot load remote content", async ({
  page,
}) => {
  await register(page);
  let remote = 0;
  await page.route("https://tracker.example/**", (route) => {
    remote++;
    return route.abort();
  });
  const body =
    '<img src="https://tracker.example/pixel">\n\n![pixel](https://tracker.example/other)\n\n[unsafe](javascript:alert%281%29)\n\n<script>alert(1)</script>';
  await newNote(page, "Untrusted Markdown", body);
  const url = page.url();
  await page.getByRole("button", { name: "Rich text", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: "Convert to rich text?" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Keep source" }).click();
  await expect(page.getByLabel("Markdown source")).toHaveValue(body);
  await page.getByRole("link", { name: "Read", exact: true }).click();
  await expect(
    page.locator('.prose img, .prose script, .prose a[href^="javascript:"]'),
  ).toHaveCount(0);
  expect(remote).toBe(0);
  await page.goto(url);
  await expect(page.getByLabel("Markdown source")).toHaveValue(body);
});
test("keyboard access and responsive authentication layout", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/sign-in");
  await expect(page.getByLabel("Email address")).toBeVisible();
  await page.getByLabel("Email address").focus();
  await page.keyboard.press("Tab");
  await expect(
    page.getByRole("link", { name: "Forgot password?" }),
  ).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByLabel("Password", { exact: true })).toBeFocused();
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/mobile-auth.png`,
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/desktop-auth.png`,
    fullPage: true,
  });
});

test("password change keeps the workspace, draft, and browser session", async ({
  page,
}) => {
  const email = await register(page);
  await newNote(page, "Password change draft", "Keep my draft");
  const url = page.url();
  await page
    .getByRole("button", { name: "Change password", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "Change password",
    exact: true,
  });
  await dialog.getByLabel("Current password", { exact: true }).fill(password);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await page
    .getByRole("button", { name: "Change password", exact: true })
    .click();
  await expect(
    dialog.getByLabel("Current password", { exact: true }),
  ).toHaveValue("");
  const replacement = "A replacement long password 789!";
  await dialog.getByLabel("Current password", { exact: true }).fill("wrong");
  await dialog.getByLabel("New password", { exact: true }).fill("short");
  await dialog
    .getByLabel("Confirm new password", { exact: true })
    .fill("short");
  await dialog
    .getByRole("button", { name: "Update password", exact: true })
    .click();
  await expect(dialog.getByRole("alert")).toContainText(
    "at least 12 characters",
  );
  await dialog.getByLabel("New password", { exact: true }).fill(replacement);
  await dialog
    .getByRole("button", { name: "Update password", exact: true })
    .click();
  await expect(dialog.getByRole("alert")).toContainText("must match");
  await dialog
    .getByLabel("Confirm new password", { exact: true })
    .fill(replacement);
  await dialog
    .getByRole("button", { name: "Update password", exact: true })
    .click();
  await expect(dialog.getByRole("alert")).toContainText("credentials invalid");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

  // Keep an unsaved draft in memory while changing credentials.
  await page.route("**/api/notes/*", (route) =>
    route.request().method() === "PATCH" ? route.abort() : route.continue(),
  );
  await page
    .getByLabel("Markdown source")
    .fill("An unsaved draft that must survive");
  await expect(page.getByText("Not saved", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "Change password", exact: true })
    .click();
  await dialog.getByLabel("Current password", { exact: true }).fill(password);
  await dialog.getByLabel("New password", { exact: true }).fill(replacement);
  await dialog
    .getByLabel("Confirm new password", { exact: true })
    .fill(replacement);
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/password-change.png`,
  });
  // An expired access cookie must refresh normally before the change is retried.
  await page.context().clearCookies({ name: "access_token" });
  await dialog
    .getByRole("button", { name: "Update password", exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  await expect(
    page.getByText("Password updated.", { exact: true }),
  ).toBeVisible();
  await expect(page).toHaveURL(url);
  await expect(page.getByLabel("Markdown source")).toHaveValue(
    "An unsaved draft that must survive",
  );
  await page.unroute("**/api/notes/*");
  await page.keyboard.press("ControlOrMeta+s");
  await expect(
    page.getByText("All changes saved", { exact: true }),
  ).toBeVisible();
  const second = await page.context().newPage();
  await second.goto(url);
  await second.getByRole("button", { name: "Markdown", exact: true }).click();
  await expect(second.getByLabel("Markdown source")).toHaveValue(
    "An unsaved draft that must survive",
  );
  await second.close();
  await page.getByRole("button", { name: "Sign out", exact: true }).click();
  await page.getByLabel("Email address").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(replacement);
  await page.getByRole("button", { name: "Sign in", exact: true }).click();
  await page.getByLabel("Verification code").fill(await code(email, "signin"));
  await page.getByRole("button", { name: "Verify & continue" }).click();
  await expect(page).toHaveURL(/\/notes$/);
});

test("rich text preserves supported Markdown and mode switches alone do not save", async ({
  page,
}) => {
  await register(page);
  await newNote(
    page,
    "Rich editing",
    "# Heading\n\n- [x] Done\n- [ ] Todo\n\n| A | B |\n| --- | --- |\n| One | Two |",
  );
  let writes = 0;
  page.on("request", (request) => {
    if (request.method() === "PATCH" && request.url().includes("/api/notes/"))
      writes++;
  });
  await page.getByRole("button", { name: "Rich text", exact: true }).click();
  const conversion = page.getByRole("dialog", {
    name: "Convert to rich text?",
  });
  if (await conversion.isVisible())
    await conversion
      .getByRole("button", { name: "Continue", exact: true })
      .click();
  await expect(
    page.getByRole("textbox", { name: "Note body", exact: true }),
  ).toBeVisible();
  await expect(page.locator('.tiptap input[type="checkbox"]')).toHaveCount(2);
  await expect(page.locator(".tiptap table")).toBeVisible();
  await page.getByRole("button", { name: "Markdown", exact: true }).click();
  await expect(page.locator(".save-status")).toContainText("All changes saved");
  expect(writes).toBe(0);
  await page.getByRole("button", { name: "Rich text", exact: true }).click();
  if (await conversion.isVisible())
    await conversion
      .getByRole("button", { name: "Continue", exact: true })
      .click();
  await page.getByRole("textbox", { name: "Note body", exact: true }).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("A new thought.");
  await page.keyboard.press("ControlOrMeta+s");
  await expect(page.locator(".save-status")).toContainText("All changes saved");
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/desktop-editor.png`,
    fullPage: true,
  });
  await page.getByRole("button", { name: "Markdown", exact: true }).click();
  expect(await page.getByLabel("Markdown source").inputValue()).toContain(
    "[x] Done",
  );
  expect(await page.getByLabel("Markdown source").inputValue()).toContain(
    "A new thought.",
  );
});
test("failed saves retain edits, guard navigation, and allow retry", async ({
  page,
}) => {
  await register(page);
  await newNote(page, "Retry test");
  await page.route("**/api/notes/*", (route) =>
    route.request().method() === "PATCH" ? route.abort() : route.continue(),
  );
  await page.getByLabel("Markdown source").fill("Do not lose this");
  await expect(page.locator(".save-status")).toContainText("Not saved");
  await page.locator(".back-link").click();
  await expect(
    page.getByRole("dialog", { name: "Your changes haven’t saved" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Stay here" }).click();
  await expect(page.getByLabel("Markdown source")).toHaveValue(
    "Do not lose this",
  );
  await page.unroute("**/api/notes/*");
  await page.getByRole("button", { name: "Retry save" }).click();
  await expect(page.locator(".save-status")).toContainText("All changes saved");
});
test("ambiguous creation is paused and cannot duplicate on further typing", async ({
  page,
}) => {
  await register(page);
  let creates = 0;
  await page.route("**/api/notes", (route) => {
    if (route.request().method() === "POST") {
      creates++;
      return route.abort();
    }
    return route.continue();
  });
  await page.getByRole("link", { name: "New note", exact: true }).click();
  await page.getByLabel("Note title").fill("Unconfirmed");
  await expect(page.getByRole("alert")).toContainText("creation was confirmed");
  await page.getByLabel("Note title").fill("Still unconfirmed");
  await page.keyboard.press("ControlOrMeta+s");
  expect(creates).toBe(1);
});

test("two tabs serialize access-token refresh through Web Locks", async ({
  page,
}) => {
  await register(page);
  await newNote(page, "Refresh coordination");
  const url = page.url();
  const second = await page.context().newPage();
  await second.goto(url);
  await expect(second.getByLabel("Note title")).toBeVisible();
  let refreshes = 0;
  page.context().on("request", (request) => {
    if (request.url().endsWith("/api/auth/refresh")) refreshes++;
  });
  await page.context().clearCookies({ name: "access_token" });
  await Promise.all([
    page.getByLabel("Search your notes").fill("coordination"),
    second.reload(),
  ]);
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await expect(second.getByLabel("Note title")).toHaveValue(
    "Refresh coordination",
  );
  expect(refreshes).toBe(1);
  await second.close();
});

test("nested folders, global search, moves, and subtree deletion", async ({
  page,
}) => {
  await register(page);
  const navigation = page.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  async function createFolder(name: string) {
    await page.getByRole("button", { name: "New folder", exact: true }).click();
    await page.getByLabel("Folder name", { exact: true }).fill(name);
    await page
      .getByRole("button", { name: "Create folder", exact: true })
      .click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }
  async function folderAction(name: string) {
    await page
      .getByRole("button", { name: "Folder actions", exact: true })
      .click();
    await page.getByRole("button", { name, exact: true }).click();
  }
  await createFolder("Projects");
  await navigation.getByRole("link", { name: "Projects", exact: true }).click();
  await createFolder("Research");
  await navigation.getByRole("link", { name: "Research", exact: true }).click();
  await newNote(page, "Nested discovery", "Searchable astronomy thought");
  await page.locator(".back-link").click();
  await expect(page).toHaveURL(/\/notes\?folder=/);
  await page.getByLabel("Search your notes", { exact: true }).fill("astronomy");
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await expect(page).toHaveURL(/q=astronomy/);
  await page.reload();
  await expect(
    page.getByLabel("Search your notes", { exact: true }),
  ).toHaveValue("astronomy");
  await page.locator(".tree-note").click();
  await expect(page.locator(".note-location")).toContainText(
    "Projects / Research",
  );
  await page.getByRole("button", { name: "Move note", exact: true }).click();
  await page.getByLabel("Destination folder", { exact: true }).selectOption("");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Move note", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(navigation.locator(".folder-tree > .tree-note")).toContainText(
    "Nested discovery",
  );
  await page.getByLabel("Clear search", { exact: true }).click();
  await navigation.getByRole("link", { name: "Research", exact: true }).click();
  await folderAction("Rename folder");
  await page.getByLabel("Folder name", { exact: true }).fill("Findings");
  await page.getByRole("button", { name: "Save folder", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await folderAction("Move folder");
  await page.getByLabel("Parent folder", { exact: true }).selectOption("");
  await page.getByRole("button", { name: "Save folder", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await folderAction("Move folder");
  const parent = await page
    .getByLabel("Parent folder")
    .locator("option")
    .filter({ hasText: "Projects" })
    .getAttribute("value");
  await page.getByLabel("Parent folder").selectOption(parent!);
  await page.getByRole("button", { name: "Save folder", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await newNote(page, "Delete with subtree");
  await page.locator(".back-link").click();
  await navigation.getByRole("link", { name: "Projects", exact: true }).click();
  await folderAction("Delete folder");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    navigation.getByRole("link", { name: "Projects", exact: true }),
  ).toBeVisible();
  await folderAction("Delete folder");
  await page
    .getByRole("button", { name: "Delete folder and contents", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    navigation.getByRole("link", { name: "Projects", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator(".tree-note").click();
  await expect(page.locator(".back-link")).toBeVisible();
  await page.locator(".back-link").click();
  await expect(navigation).toBeVisible();
});

test("folder deletion preserves a conflicting draft and stale folder actions require refresh", async ({
  page,
}) => {
  await register(page);
  await page.getByRole("button", { name: "New folder", exact: true }).click();
  await page.getByLabel("Folder name", { exact: true }).fill("Draft home");
  await page
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page
    .getByRole("navigation", { name: "Folders", exact: true })
    .getByRole("link", { name: "Draft home", exact: true })
    .click();
  await newNote(page, "Keep my draft");
  const second = await page.context().newPage();
  await second.goto(page.url());
  await second
    .getByLabel("Note title", { exact: true })
    .fill("Changed remotely");
  await expect(second.locator(".save-status")).toContainText(
    "All changes saved",
  );
  await page
    .getByLabel("Note title", { exact: true })
    .fill("Local unsaved content");
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete folder", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete folder and contents", exact: true })
    .click();
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText(
    "Save or resolve",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByLabel("Note title", { exact: true })).toHaveValue(
    "Local unsaved content",
  );
  await expect(
    page.getByRole("heading", {
      name: "This note changed elsewhere",
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Load latest", exact: true }).click();
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Rename folder", exact: true })
    .click();
  // Keep the dialog's original revision while the other tab changes the folder.
  await second
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await second
    .getByRole("button", { name: "Rename folder", exact: true })
    .click();
  await second
    .getByLabel("Folder name", { exact: true })
    .fill("Remote folder name");
  await second
    .getByRole("button", { name: "Save folder", exact: true })
    .click();
  await expect(second.getByRole("dialog")).toHaveCount(0);
  await page
    .getByLabel("Folder name", { exact: true })
    .fill("Stale folder name");
  await page.getByRole("button", { name: "Save folder", exact: true }).click();
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText(
    "changed elsewhere",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page
      .getByRole("navigation", { name: "Folders", exact: true })
      .getByRole("link", { name: "Remote folder name", exact: true }),
  ).toBeVisible();
  await second.close();
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete folder", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Delete folder and contents", exact: true })
    .click();
  await expect(page).toHaveURL(/\/notes$/);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByText("A fresh start", { exact: true })).toBeVisible();
});

test("code examples open in rich text and library previews hide Markdown syntax", async ({
  page,
}) => {
  await register(page);
  await newNote(
    page,
    "Readable code example",
    "# Checklist\n\nRead [the project](https://example.com).\n\n- [x] Done\n\n```html\n<div>literal example</div>\n```",
  );
  await page.getByRole("link", { name: "Read", exact: true }).click();
  await expect(page.locator(".prose ul")).toHaveCSS("list-style-type", "none");
  await page.getByRole("link", { name: "Edit note", exact: true }).click();
  await expect(
    page.getByRole("textbox", { name: "Note body", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".tiptap pre")).toContainText(
    "<div>literal example</div>",
  );
  await page.locator(".back-link").click();
  await expect(page.locator(".tree-note")).toContainText("Read the project.");
  await expect(page.locator(".tree-note")).not.toContainText("https://");
});

test("long editor titles wrap on mobile and resize with the viewport", async ({
  page,
}) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.setViewportSize({ width: 390, height: 844 });
  await register(page);
  const title =
    "Café project — a long launch plan with enough detail to wrap across multiple lines";
  await newNote(page, title);
  const field = page.getByLabel("Note title");
  await expect(field).toHaveValue(title);
  const mobile = await field.evaluate((element) => ({
    height: element.clientHeight,
    content: element.scrollHeight,
  }));
  expect(mobile.height).toBeGreaterThan(70);
  expect(mobile.content).toBeLessThanOrEqual(mobile.height + 1);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await expect
    .poll(() => field.evaluate((element) => element.clientHeight))
    .toBeLessThan(mobile.height);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(() => field.evaluate((element) => element.clientHeight))
    .toBe(mobile.height);
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/mobile-wrapped-title.png`,
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test("long folder names keep navigation, note results, and reader actions usable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await register(page);
  const folderName = "Product research — customer interviews and planning";
  const folders = page.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  await page.getByRole("button", { name: "New folder", exact: true }).click();
  await page.getByLabel("Folder name", { exact: true }).fill(folderName);
  await page
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await folders.getByRole("link", { name: folderName, exact: true }).click();
  const folderMenu = page.getByRole("button", {
    name: "Folder actions",
    exact: true,
  });
  await expect(
    page.getByRole("button", { name: "Rename folder", exact: true }),
  ).toBeHidden();
  await folderMenu.click();
  await expect(
    page.getByRole("button", { name: "Rename folder", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(folderMenu).toBeFocused();
  await expect(
    page.getByRole("button", { name: "Rename folder", exact: true }),
  ).toBeHidden();
  await folderMenu.click();
  await page.getByLabel("Search your notes", { exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Rename folder", exact: true }),
  ).toBeHidden();
  const collapse = await page
    .getByRole("button", { name: "Collapse sidebar", exact: true })
    .boundingBox();
  const create = await page
    .getByRole("link", { name: "New note", exact: true })
    .boundingBox();
  expect(collapse!.x).toBeGreaterThan(create!.x + create!.width);
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page
    .getByRole("button", { name: "New subfolder", exact: true })
    .click();
  await page.getByLabel("Folder name", { exact: true }).fill("Interview notes");
  await page
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByText("No notes in this folder", { exact: true }),
  ).toHaveCount(0);
  await folders
    .getByRole("link", { name: "Interview notes", exact: true })
    .click();
  await newNote(
    page,
    "A long note title that should remain visible",
    "Keep the local draft when collapsing navigation.",
  );
  const list = page.getByRole("navigation", { name: "Folders", exact: true });
  expect(
    await list.evaluate((el) => el.getBoundingClientRect().height),
  ).toBeGreaterThanOrEqual(120);
  await page
    .getByLabel("Note title", { exact: true })
    .fill("Updated while collapsing the sidebar");
  await page
    .getByRole("button", { name: "Collapse sidebar", exact: true })
    .click();
  await expect(folders).toBeHidden();
  await expect(
    page.getByRole("button", { name: "Expand sidebar", exact: true }),
  ).toBeFocused();
  await expect(page.getByLabel("Note title", { exact: true })).toHaveValue(
    "Updated while collapsing the sidebar",
  );
  await page
    .getByRole("button", { name: "Expand sidebar", exact: true })
    .click();
  await expect(folders).toBeVisible();
  await folders.getByRole("link", { name: folderName, exact: true }).click();
  await page.getByLabel("Search your notes", { exact: true }).fill("local");
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await page.setViewportSize({ width: 390, height: 844 });
  const navigationWidth = await folders.evaluate((el) => ({
    width: el.clientWidth,
    contentWidth: el.scrollWidth,
    left: el.scrollLeft,
  }));
  expect(navigationWidth.contentWidth).toBeLessThanOrEqual(
    navigationWidth.width,
  );
  expect(navigationWidth.left).toBe(0);
  await page.locator(".tree-note").click();
  await expect(
    page.getByRole("link", { name: "Edit note", exact: true }),
  ).toBeInViewport();
  await expect(
    page.getByRole("button", { name: "Delete note", exact: true }),
  ).toBeInViewport();
  const readerWidth = await page.getByRole("main").evaluate((el) => ({
    width: el.clientWidth,
    contentWidth: el.scrollWidth,
  }));
  expect(readerWidth.contentWidth).toBeLessThanOrEqual(readerWidth.width);
});

test("a failed rich editor download falls back to editable Markdown", async ({
  page,
}) => {
  await register(page);
  await page.route("**/src/lib/components/Editor.svelte*", (route) =>
    route.abort("failed"),
  );
  await page.getByRole("link", { name: "New note", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(
    "rich text editor could not load",
  );
  await page.getByLabel("Note title", { exact: true }).fill("Fallback note");
  await page
    .getByLabel("Markdown source", { exact: true })
    .fill("# Still editable\n\nMy draft survives a failed download.");
  await expect(page).toHaveURL(/\/notes\/[^/?]+\/edit(?:\?|$)/);
  await expect(page.locator(".save-status")).toContainText("All changes saved");
  await page.getByRole("link", { name: "Read", exact: true }).click();
  await expect(page.locator(".prose")).toContainText(
    "My draft survives a failed download.",
  );
});

test("notes outside folders appear directly at the top level", async ({
  page,
}) => {
  await register(page);
  const navigation = page.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  await page.getByRole("button", { name: "New folder", exact: true }).click();
  await page.getByLabel("Folder name", { exact: true }).fill("Projects");
  await page
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await newNote(page, "Loose thought");
  await expect(page.locator(".note-location")).toHaveText("Top level");
  const note = navigation.locator(".folder-tree > .tree-note");
  await expect(note).toContainText("Loose thought");
  await expect(
    navigation.getByRole("link", { name: "Unfiled", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Move note", exact: true }).click();
  await expect(
    page.getByLabel("Destination folder").locator('option[value=""]'),
  ).toHaveText("Top level");
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await navigation.getByRole("link", { name: "Projects", exact: true }).click();
  await expect(note).toContainText("Loose thought");
  await note.click();
  await expect(page.locator(".read-title")).toHaveText("Loose thought");
  await page.getByLabel("Search your notes", { exact: true }).fill("Loose");
  await expect(note).toContainText("Loose thought");
  await page
    .getByLabel("Search your notes", { exact: true })
    .fill("absentword");
  await expect(note).toHaveCount(0);
});

test("unified sidebar filters nested branches and highlights plaintext content previews", async ({
  page,
}) => {
  await register(page);
  const navigation = page.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  async function createFolder(name: string) {
    await page.getByRole("button", { name: "New folder", exact: true }).click();
    await page.getByLabel("Folder name", { exact: true }).fill(name);
    await page
      .getByRole("button", { name: "Create folder", exact: true })
      .click();
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }
  await createFolder("Projects");
  await navigation.getByRole("link", { name: "Projects", exact: true }).click();
  await createFolder("Research");
  await navigation.getByRole("link", { name: "Research", exact: true }).click();
  await newNote(
    page,
    "Astronomy journal",
    "Opening context. ".repeat(25) +
      "**Astronomy** findings from [observations](https://example.com).",
  );
  await page.locator(".back-link").click();
  await navigation
    .getByRole("link", { name: "All notes", exact: true })
    .click();
  await createFolder("Unrelated");
  await page
    .getByRole("button", { name: "Collapse Projects", exact: true })
    .click();
  await expect(page.locator(".tree-note")).toHaveCount(0);
  await page.getByLabel("Search your notes", { exact: true }).fill("astronomy");
  await expect(page.locator(".tree-note")).toHaveCount(1);
  await expect(
    navigation.getByRole("link", { name: "Projects", exact: true }),
  ).toBeVisible();
  await expect(
    navigation.getByRole("link", { name: "Research", exact: true }),
  ).toBeVisible();
  await expect(
    navigation.getByRole("link", { name: "Unrelated", exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".tree-note-title strong")).toHaveText("Astronomy");
  await expect(page.locator(".tree-note-preview strong")).toHaveText(
    "Astronomy",
  );
  await expect(page.locator(".tree-note-preview")).toContainText(
    "findings from observations.",
  );
  await expect(page.locator(".results-panel")).toHaveCount(0);
  await page.locator(".tree-note").click();
  await expect(page.locator(".read-title")).toHaveText("Astronomy journal");
  await expect(page.locator(".tree-note")).toHaveClass(/selected/);
  await expect(page.locator(".tree-status")).toHaveCount(0);
  const treeSize = await navigation.evaluate((el) => ({
    height: el.clientHeight,
    content: el.scrollHeight,
  }));
  expect(treeSize.content).toBeLessThanOrEqual(treeSize.height);
  await page.screenshot({
    path: "test-results/unified-search-desktop.png",
    fullPage: true,
  });
  await page
    .getByLabel("Search your notes", { exact: true })
    .fill("absentword");
  await expect(page.getByText("No notes found", { exact: true })).toBeVisible();
  await expect(
    navigation.getByRole("link", { name: "Projects", exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Clear search", { exact: true }).click();
  await expect(
    navigation.getByRole("link", { name: "Unrelated", exact: true }),
  ).toBeVisible();
  await page.locator(".back-link").click();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByLabel("Search your notes", { exact: true }).fill("astronomy");
  await expect(page.locator(".tree-note-preview strong")).toHaveText(
    "Astronomy",
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/unified-search-mobile.png",
    fullPage: true,
  });
});

test("sharing a note by name supports view, edit, and revoke", async ({
  page,
  browser,
}) => {
  test.setTimeout(60000);
  await register(page, "Morgan Owner");
  const recipientContext = await browser.newContext();
  const recipient = await recipientContext.newPage();
  const email = await register(recipient, "Riley Recipient");
  await newNote(page, "A private shared note", "Original shared content");
  const noteUrl = page.url();
  await page.getByRole("button", { name: "Share note", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Share note", exact: true });
  await expect(dialog.getByLabel("Access for new recipients")).toHaveValue(
    "view",
  );
  await dialog.getByLabel("Find a person by name or email").fill("riley");
  await dialog
    .getByRole("button", { name: `Share with ${email}`, exact: true })
    .click();
  await expect(
    dialog.getByRole("list", { name: "People with access" }),
  ).toContainText(email);
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/share-note-desktop.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    dialog.getByRole("button", { name: "Done", exact: true }),
  ).toBeInViewport();
  await page.screenshot({
    path: `${process.env.E2E_SCREENSHOT_DIR || "test-results"}/share-note-mobile.png`,
    fullPage: true,
  });
  await page.setViewportSize({ width: 1280, height: 720 });
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await recipient
    .getByRole("navigation", { name: "Folders", exact: true })
    .getByRole("link", { name: "Shared with me", exact: true })
    .click();
  await recipient
    .locator(".tree-note")
    .filter({ hasText: "A private shared note" })
    .click();
  await expect(recipient.locator(".shared-banner")).toContainText(
    "Morgan Owner · Can view",
  );
  await expect(
    recipient.getByRole("link", { name: "Edit note", exact: true }),
  ).toHaveCount(0);
  await expect(
    recipient.getByRole("button", { name: "Delete note", exact: true }),
  ).toHaveCount(0);
  await expect(
    recipient.getByRole("button", { name: "Share note", exact: true }),
  ).toHaveCount(0);
  await recipient.goto(noteUrl);
  await expect(recipient.getByLabel("Note title", { exact: true })).toHaveCount(
    0,
  );
  await expect(recipient.locator(".prose")).toContainText(
    "Original shared content",
  );
  await page.getByRole("button", { name: "Share note", exact: true }).click();
  await dialog
    .getByLabel(`Access for ${email}`, { exact: true })
    .selectOption("edit");
  await expect(dialog.getByRole("status")).toContainText("Access updated");
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await recipient.reload();
  await recipient
    .getByLabel("Note title", { exact: true })
    .fill("Edited by recipient");
  await recipient
    .getByRole("button", { name: "Markdown", exact: true })
    .click();
  await recipient
    .getByLabel("Markdown source", { exact: true })
    .fill("A recipient can now edit this note.");
  await expect(recipient.locator(".save-status")).toContainText(
    "All changes saved",
  );
  await page.reload();
  await expect(page.getByLabel("Note title", { exact: true })).toHaveValue(
    "Edited by recipient",
  );
  await page.getByRole("button", { name: "Share note", exact: true }).click();
  await dialog
    .getByRole("button", { name: `Remove access for ${email}`, exact: true })
    .click();
  await expect(
    dialog.getByRole("list", { name: "People with access" }),
  ).not.toContainText(email);
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await recipient.reload();
  await expect(
    recipient.getByRole("heading", {
      name: "Unable to open note",
      exact: true,
    }),
  ).toBeVisible();
  await recipientContext.close();
});

test("sharing a folder by email includes future notes and subfolders", async ({
  page,
  browser,
}) => {
  test.setTimeout(60000);
  await register(page, "Taylor Folder Owner");
  const recipientContext = await browser.newContext();
  const recipient = await recipientContext.newPage();
  const email = await register(recipient, "Jordan Collaborator");
  const folders = page.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  await page.getByRole("button", { name: "New folder", exact: true }).click();
  await page.getByLabel("Folder name", { exact: true }).fill("Shared project");
  await page
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await folders
    .getByRole("link", { name: "Shared project", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page.getByRole("button", { name: "Share folder", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "Share folder",
    exact: true,
  });
  await dialog.getByLabel("Find a person by name or email").fill(email);
  await dialog.getByLabel("Access for new recipients").selectOption("edit");
  await dialog
    .getByRole("button", { name: `Share with ${email}`, exact: true })
    .click();
  await expect(
    dialog.getByRole("list", { name: "People with access" }),
  ).toContainText(email);
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  const received = recipient.getByRole("navigation", {
    name: "Folders",
    exact: true,
  });
  await received
    .getByRole("link", { name: "Shared with me", exact: true })
    .click();
  await received
    .getByRole("link", { name: "Shared project", exact: true })
    .click();
  await newNote(
    recipient,
    "Created by collaborator",
    "Future content inherits access.",
  );
  await expect(recipient.locator(".shared-banner")).toContainText(
    "Taylor Folder Owner · Can edit",
  );
  await expect(
    recipient.getByRole("button", { name: "Move note", exact: true }),
  ).toHaveCount(0);
  await recipient
    .getByRole("button", { name: "New folder", exact: true })
    .click();
  await recipient
    .getByLabel("Folder name", { exact: true })
    .fill("Collaborator subfolder");
  await recipient
    .getByRole("button", { name: "Create folder", exact: true })
    .click();
  await expect(recipient.getByRole("dialog")).toHaveCount(0);
  await expect(
    received.getByRole("link", { name: "Collaborator subfolder", exact: true }),
  ).toBeVisible();
  await page.reload();
  await folders
    .getByRole("link", { name: "Shared project", exact: true })
    .click();
  await expect(page.locator(".tree-note")).toContainText(
    "Created by collaborator",
  );
  await page
    .getByRole("button", { name: "Folder actions", exact: true })
    .click();
  await page.getByRole("button", { name: "Share folder", exact: true }).click();
  await dialog
    .getByRole("button", { name: `Remove access for ${email}`, exact: true })
    .click();
  await expect(dialog.getByRole("status")).toContainText(
    "Direct access removed",
  );
  await dialog.getByRole("button", { name: "Done", exact: true }).click();
  await recipient.goto("/notes?folder=shared");
  await expect(
    received.getByRole("link", { name: "Shared project", exact: true }),
  ).toHaveCount(0);
  await expect(recipient.locator(".tree-note")).toHaveCount(0);
  await recipientContext.close();
});

test("an existing account can complete and update its name", async ({
  page,
}) => {
  await register(page, "Original Name");
  await page.route(
    "**/api/user/me",
    async (route) => {
      if (route.request().method() !== "GET") return route.continue();
      const response = await route.fetch();
      await route.fulfill({
        response,
        json: { ...(await response.json()), name: "" },
      });
    },
    { times: 1 },
  );
  await page.reload();
  const dialog = page.getByRole("dialog", {
    name: "Add your name",
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Full name", { exact: true }).fill("Updated Name");
  await dialog
    .getByRole("button", { name: "Save profile", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(".account-email")).toContainText("Updated Name");
  await page.reload();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".account-email")).toContainText("Updated Name");
});
