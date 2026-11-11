# Bluestaq Notes frontend

Svelte 5 / SvelteKit 3 application for private Markdown notes. The interface adapts the warm neutral surfaces, teal accents, typography, and editor patterns from the owned `slp-website` frontend.

## Local development

Start the Rust API using `backend/README.md`. For local HTTP, set `SECURE_COOKIES=false` in the backend configuration. Then:

```sh
pnpm install
pnpm dev
```

Vite forwards `/api` to `http://127.0.0.1:8080`; override this with the server-only `API_PROXY_TARGET` environment variable. Open the Vite URL, rather than opening the API on another origin. Email verification codes are printed by the backend's development mock email sender.

## Application

- `/register`, `/sign-in`, `/recover`: registration verification, mandatory email 2FA, and password recovery. Codes expire after ten minutes; resend replaces the challenge after a sixty-second cooldown.
- `/notes`: paginated personal library with nested folders and title-weighted full-text search.
- `?folder=<id>` selects a folder; `?folder=root` remains supported for top-level notes; `?q=<query>` preserves search. Context is retained in note links and back navigation.
- `/notes/new`, `/notes/:id`, `/notes/:id/edit`: creation, reading, and editing.

The folder panel supports nested creation, renaming, moving, and permanent subtree deletion with confirmation. Notes outside folders appear directly at the top level of the tree, alongside folders. All notes searches globally. New notes use the selected folder; note moves participate in autosave and revision conflict resolution.

The unified sidebar shows folders and note previews in one tree. Search filters matching branches and highlights plaintext matches. Mobile navigation switches between the browser and document.

Rich text uses Tiptap's official Markdown integration. Supported content includes headings, emphasis, strikethrough, lists, task lists, blockquotes, links, code, horizontal rules, and tables. Markdown source is stored directly in the API. Source mode retains exact text; switching modes does not save. Conversion can normalize formatting, so conversion differences require confirmation. Unsupported HTML/images start in source mode. Only bare `<br>` tags are supported for table-cell line breaks. Other embedded HTML cannot execute; remote images are never loaded, and links allow HTTP, HTTPS, or mailto only.

Autosave runs after 800 ms without input and serializes writes per draft. Ctrl/Cmd+S saves immediately. Revision conflicts pause saving and retain the draft for comparison, replacement, or creation as a separate note. Failed writes retain edits in memory and offer retry. Ambiguous creation failures are paused: check the library before creating another copy. Navigation flushes pending changes or asks before discarding. Closing/reloading with unsaved content warns, but drafts cannot survive a browser shutdown or forced refresh.

Expired sessions hide the workspace behind same-account reauthentication. Note content, credentials, codes, and session tokens are never persisted in browser storage. Only display preferences are stored. Session cookies remain HTTP-only; the readable CSRF cookie is copied into unsafe requests. Refresh calls are serialized with Web Locks across tabs (and a shared promise within each tab). Browsers without Web Locks have within-tab serialization only; concurrent cross-tab rotation can require signing in again.

## Checks

```sh
pnpm check
pnpm test:unit
pnpm exec playwright install chromium
pnpm test:e2e
pnpm build
```

Unit tests cover API/CSRF behavior, refresh coordination, Markdown safety/import, and autosave races/failures/conflicts. Playwright starts isolated frontend/backend servers on ports 15173/18080 with a temporary database and temporary mock-mail log, exercises real authentication and CRUD, and captures desktop/mobile screenshots. Its inbox reader is test-process filesystem access; no email-code endpoint exists. Test fixtures and traces can contain test note content; all use generated disposable accounts.

## Production

Serve the frontend and `/api` on the **same HTTPS origin**, forwarding `/api` to Rust with cookies, `x-xsrf-token`, `If-Match`, and `ETag` intact. The Vite development proxy is not production hosting. Select the SvelteKit adapter appropriate to your deployment; the current scaffold retains adapter-auto. Configure `SECURE_COOKIES=true`, a real email sender, stable signing/pepper keys, and TLS/HSTS at the reverse proxy.

SvelteKit sends CSP, no-store, nosniff, no-referrer, frame denial, and restricted permissions headers. Keep these headers at the edge as well; never cache authenticated API responses. The CSP allows inline styles needed by the editor, but no third-party scripts or remote fonts. No service worker or offline note cache is installed. The backend's existing short-lived access-token semantics remain unchanged.


## Profiles and sharing

Sign-up requires a full name. Existing users with no name see a profile-completion dialog, and the account footer offers **Edit profile**. The owner’s **Share note** toolbar button and folder **Share folder** action open a sharing dialog. Search registered users by name or email, choose Can view (default) or Can edit, and manage or remove existing direct grants.

**Shared with me** shows received notes and folders. Shared folders include current and future descendants; hidden private ancestors are not displayed. Shared content identifies its owner and access level. Viewers see a reader; editors can edit notes, rename folders, and add content inside editable shared folders. Moves, deletion, and resharing remain owner-only. Revocation is enforced by the API, and failed/revoked saves retain the local draft.
