# Bluestaq Notes API

One Cargo package with an Actix Web API. SQLite is compiled into the application, so no system SQLite installation is needed. Diesel manages the database; migrations run automatically at startup. The Svelte frontend connects through same-origin `/api` requests.

## Running

From `backend/`, install Rust/Cargo and a C compiler (for example, Xcode Command Line Tools on macOS), then:

```sh
cp sample.env .env
```

Generate **two separate keys** with `dd if=/dev/urandom bs=32 count=1 2>/dev/null | base64` and put them in `TOKEN_SIGNING_KEY_B64` and `PASSWORD_PEPPER_B64` in `.env`. Both must decode to at least 32 bytes. Keep them stable across restarts: changing the signing key invalidates existing access tokens; changing the pepper prevents existing password hashes from verifying.

```sh
cargo run
```

The API defaults to `127.0.0.1:8080` and stores data in `notes.sqlite`. Set `DATABASE_URL` to change the database path. SQLite uses WAL, foreign keys, a five-second busy timeout, and an eight-connection pool. Do not use `:memory:` with this pool; tests use temporary database files.

`MockSender` prints email messages and codes to the API terminal; nothing is delivered externally. This is development email delivery. The `SendEmail` interface can accept a real SMTP sender later. Normal request logs exclude query strings, bodies, tokens, and cookies.

`SECURE_COOKIES=false` enables local HTTP development; use `true` with HTTPS. The default when omitted is `true`. Serve the eventual frontend and API on the same origin (for example, proxy `/api` through the frontend server). No cross-origin access is enabled.

## Authentication

Passwords have a minimum of 12 characters and a maximum of 1024 UTF-8 bytes. They are sent directly over HTTPS and hashed on the server with Argon2id’s native secret/pepper support, random 16-byte salts, 32 MiB memory, and four iterations. Password verification uses `verify_with_secret`; there is no HMAC preprocessing. There is no client hashing, keystore, or E2EE.

1. Obtain a CSRF cookie with `GET /api/auth/csrf-token`: `__Host-xsrf-token` over HTTPS, or `xsrf-token` when `SECURE_COOKIES=false` for local HTTP.
2. Send that cookie value in `x-xsrf-token` for **every unsafe request**, including registration and sign-in. `actix-csrf` validates the double-submit cookie/header pair without server-side token storage.
3. Register with name, email, and password. Registration returns `202` with `{challenge_id, purpose: "registration", expires_in: 600}`; it does not issue an access token yet.
4. Read the code in the mock email output. Submit `{challenge_id, code}` to `/api/auth/register/verify`. Success sets `access_token` and `refresh_token` cookies and returns `{user: {id, email, name}}`.
5. Later sign-ins return a challenge with purpose `signin`. Submit it and the emailed code to `/api/auth/2fa/email/verify` before accessing notes. Sign-in to a pending account instead returns a registration challenge.

Both auth cookies are host-only, HTTP-only, SameSite Strict, and scoped to `/`. `AuthenticatedUser` verifies access tokens locally without a database lookup. Access tokens default to five minutes (`ACCESS_TOKEN_LIFETIME_SECONDS=300`); refresh tokens default to seventy-two hours (`REFRESH_TOKEN_LIFETIME_SECONDS=259200`). Both settings must be positive integers. Normal requests do not renew access tokens.

Call `POST /auth/refresh` with the refresh cookie and CSRF header to obtain a new pair, including after the access token expires. The old refresh token is atomically blacklisted and cannot be used twice; serialize refresh calls in the client. Tokens have distinct types, and refresh tokens contain random nonces so rotations within the same second still produce different tokens. Both lifetimes remain capped by the original thirty-day login deadline; refreshing never extends that deadline. If the refresh token expires, sign in with password and email code again.

Only the CSRF cookie is readable by JavaScript. It is retained across refreshes and cleared on logout or password reset. Fetch `/auth/csrf-token` again if it is missing. Logout and password reset blacklist the refresh token presented by this browser and clear all three cookies. Copied access tokens remain usable until their short expiration. Password reset increments the user’s database `token_version`, invalidating every previously issued refresh token. Logout revokes only the presented refresh token. Signing-key rotation invalidates all tokens. There is no session table; `/user/me` loads the current profile from the database; authentication still verifies access tokens locally. Refresh requests compare the token’s signed version with the current user row, including a recheck inside the blacklist transaction. The version persists across restarts and works across instances sharing the database. Access validation does not query it. All API instances must use the same signing key and database.

The refresh blacklist stores token fingerprints and their expiration times. Each insertion checks a shared in-memory `last_blacklist_clear_time`, initialized at process startup. Once more than six hours have passed since the last successful cleanup, count rows whose expiry is strictly earlier than `now - refresh-token lifetime`. Delete those rows only when the count is greater than 100, then advance the timestamp after commit. Skipped checks and failed transactions do not advance it. No job runner or persisted cleanup timestamp is needed; restarting merely delays the next check.

Email codes are eight case-insensitive alphanumeric characters, expire after ten minutes, and permit five attempts. Resend requires sixty seconds, replaces the challenge identifier, and invalidates the previous code. A challenge can be used only for its specified purpose and only once. Codes are hashed in the database and never returned by the API.

Reset requests return the same `202` response shape whether or not an account exists (with an opaque placeholder challenge when no email is sent or the request is on cooldown). Submit `{challenge_id, code, new_password}` to confirm a reset. It invalidates outstanding email challenges, clears this browser’s cookies, and requires signing in again in that browser. The database token version is incremented atomically with the password update, so all old refresh tokens are rejected, including those from other browsers. The presented refresh token is also blacklisted. Resetting a password does not issue an access token or bypass sign-in 2FA.

Password and email-code hashing/verification run on Rayon’s CPU pool; Diesel connection checkout, queries, and transactions run through `web::block`. Database connections are released before awaiting expensive crypto. At most `HASH_CONCURRENCY` Argon2 operations run simultaneously per service instance (default 2); excess requests receive `429`.

Signed-in users can call `POST /api/auth/password-change` with `{current_password, new_password}` and the CSRF header. Success returns `200` with `{user}` and replacement access/refresh cookies, preserving the CSRF cookie and original thirty-day login deadline. The password update, refresh-token version increment, outstanding-challenge deletion, and replacement-token generation are atomic. All older refresh tokens are rejected; existing access tokens remain valid until their short expiration. The current browser stays signed in. Password change requires the current password and does not send an email code. Forgot-password recovery remains unchanged.

## Endpoints

All paths are prefixed with `/api`. Requests and responses use JSON. Application errors use `{"error":"message"}`; malformed requests and unmatched routes use Actix’s default error responses. Verification failures return `401`, CSRF failures `422`, inaccessible notes `404`, and limits `429`.

| Method | Path | Request / behavior |
| --- | --- | --- |
| GET | `/health` | Process liveness |
| GET | `/auth/csrf-token` | Set CSRF cookie |
| POST | `/auth/register` | `{name, email, password}`; pending account, `202` |
| POST | `/auth/register/verify` | `{challenge_id, code}`; access cookie, `200` |
| POST | `/auth/sign-in` | `{email, password}`; email challenge, `202` |
| POST | `/auth/2fa/email/verify` | `{challenge_id, code}`; access cookie, `200` |
| POST | `/auth/email/resend` | `{challenge_id}`; replacement challenge, `202` |
| POST | `/auth/refresh` | Refresh cookie; rotate tokens, `200` |
| POST | `/auth/logout` | Blacklist presented refresh token and clear cookies, `204` |
| POST | `/auth/password-change` | `{current_password, new_password}`; authenticated, fresh session cookies, `200` |
| POST | `/auth/password-reset/request` | `{email}`; generic challenge response, `202` |
| POST | `/auth/password-reset/confirm` | `{challenge_id, code, new_password}`; `204` |
| GET | `/user/me` | Current `{id, email, name}` |
| POST | `/notes` | `{title?, body?}`; note, `201` |
| GET | `/notes?limit=50&offset=0` | Array of owned notes; limit 1–100 |
| GET | `/notes/{id}` | Owned note, `200` |
| PATCH | `/notes/{id}` | `{title?, body?}`; at least one field; note, `200` |
| DELETE | `/notes/{id}` | Permanent deletion, `204` |

Notes have `{id, title, body, created_at, updated_at}`. Timestamps are Unix seconds. Empty content is allowed; title is limited to 255 Unicode characters and body to 100 KiB of UTF-8. Listing sorts by most recently updated, then ID for stable pagination. Ownership is derived from the signed access token; there is no sharing or client-supplied owner field. JSON requests are capped at 4 MiB; the note body limit remains 100 KiB.

## Example requests

From a local shell with the API running:

```sh
curl -c /tmp/bluestaq-cookies http://127.0.0.1:8080/api/auth/csrf-token
csrf=$(awk '$6 == "xsrf-token" { print $7 }' /tmp/bluestaq-cookies)
curl -b /tmp/bluestaq-cookies -c /tmp/bluestaq-cookies \
  -H "x-xsrf-token: $csrf" -H 'Content-Type: application/json' \
  -d '{"name":"Example User","email":"you@example.com","password":"a long example password"}' \
  http://127.0.0.1:8080/api/auth/register
```

Use the returned challenge ID and the code printed in the API terminal:

```sh
curl -b /tmp/bluestaq-cookies -c /tmp/bluestaq-cookies \
  -H "x-xsrf-token: $csrf" -H 'Content-Type: application/json' \
  -d '{"challenge_id":"REPLACE_WITH_ID","code":"REPLACE_WITH_CODE"}' \
  http://127.0.0.1:8080/api/auth/register/verify
curl -b /tmp/bluestaq-cookies \
  -H "x-xsrf-token: $csrf" -H 'Content-Type: application/json' \
  -d '{"title":"First note","body":"Hello"}' http://127.0.0.1:8080/api/notes
curl -b /tmp/bluestaq-cookies http://127.0.0.1:8080/api/notes
```

## Checks

The API enforces token and challenge expiration when handling requests. Expired records remain stored in SQLite.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The initial migration includes `token_version`. If you have a disposable database from an earlier scaffold, recreate it before running this schema; old HMAC-preprocessed password hashes also require a password reset or account recreation.

Tests use isolated temporary SQLite files, fixed test keys, and the mock inbox. They require neither `.env` nor an email service. Tests live in a single bottom-of-file `#[cfg(test)] mod tests` block beside the code they exercise. Shared fixtures and request helpers live in the private, test-only `src/test_utils.rs` module. Handler tests issue Actix requests through the real routes; service tests call services directly against migrated SQLite databases. Each test owns its database, so the suite runs in parallel.

## Note revisions and search

Note responses now include `revision`, initially `1`. Creation, individual reads, and updates return a strong `ETag`, for example `"1"`. PATCH and DELETE require the current ETag in `If-Match: "1"`; missing headers return `428`, malformed values return `400`, and stale revisions return `412`. Each successful PATCH increments the revision atomically, even for two updates within the same second. A stale DELETE never removes the changed note. Unowned notes remain inaccessible with `404`. Clients must reload and resolve conflicts before replacing content.

`GET /api/notes/search?q=project&limit=50&offset=0` returns `{items, has_more}`. Empty queries use recent-note ordering. Nonempty queries match literal Unicode words across title/body, require every term, and prefix-match the final term. Matching ignores case and diacritics. Exact title phrases rank first, followed by BM25 with title/body weights `8:1`, then updated time and ID. Search is lexical, without typo correction or embeddings. Punctuation-only input returns no results. Queries allow 256 Unicode characters and 20 terms, with the usual pagination bounds. Quotes and operators are treated as input text, not FTS syntax.

The initial migration includes revisions, folders, sharing, and a SQLite FTS5 index maintained by database triggers. The bundled SQLite build supplies FTS5. Tests cover fresh databases and reopening a database with stored notes. Recreate disposable databases that used the earlier migration history before running this schema.

Example conditional update (replace `NOTE_ID` and use the ETag from its latest read):

```sh
curl -b /tmp/bluestaq-cookies -c /tmp/bluestaq-cookies \
  -H "x-xsrf-token: $csrf" -H 'Content-Type: application/json' \
  -H 'If-Match: "1"' -X PATCH -d '{"body":"Updated **Markdown**"}' \
  http://127.0.0.1:8080/api/notes/NOTE_ID
```

## Folders

Folders and notes start private and can be shared with registered users. `GET /api/folders` returns a flat array of `{id, name, parent_id, created_at, updated_at, revision, access, owner}`; `parent_id: null` denotes a top-level folder. There is no application-imposed depth limit. Names are trimmed and must contain 1–255 Unicode characters; duplicate names are allowed.

| Method | Endpoint | Body / behavior |
| --- | --- | --- |
| POST | `/api/folders` | `{name, parent_id?}`; returns 201 with folder, Location, and ETag |
| PATCH | `/api/folders/:id` | `{name?, parent_id?}`; returns updated folder and ETag |
| DELETE | `/api/folders/:id` | Permanently removes the current subtree and its notes atomically; returns 204 |

PATCH and DELETE require the folder's quoted revision in `If-Match`, with the same 428/400/412 precondition behavior as notes. Subtree deletion checks the selected folder's revision, rather than a snapshot of descendant revisions. All writes require CSRF. Missing or inaccessible folders/destinations return 404; invalid names, empty patches, and moves into self/descendants return 400. Omitted parent fields preserve the relationship; explicit `parent_id: null` moves to the top level.

Notes now return `folder_id: string | null`. POST accepts an optional folder ID; PATCH accepts a folder ID to move or explicit null to unfile, protected by the note's existing revision. Omitting the field leaves its folder unchanged.

Both `/api/notes` and `/api/notes/search` accept optional `folder_id` and `include_descendants` query parameters. Without `folder_id`, results cover all notes. `folder_id=root` selects only unfiled notes. A folder UUID selects its immediate notes by default; `include_descendants=true` includes all descendant folders. Folder restrictions apply before pagination and search ranking. Existing notes are migrated to Unfiled without changes to content, timestamps, or revisions; search indexing remains synchronized after moves and deletion.


## Profiles and sharing

Registration requires a trimmed `name` of 1–100 Unicode characters in addition to email and password. Existing accounts migrate with an empty name and can complete their profile using `PATCH /api/user/me` with `{name}`. Profile and sign-in responses include `{id, email, name}`; signing cookies remain compatible across the migration.

Authenticated `GET /api/users/search?q=...` searches verified users by case-insensitive name or email substring. Search text must contain 2–100 characters. Literal `%`, `_`, and backslashes are escaped. Results contain only ID, name, and email, exclude the requester and pending accounts, and are capped at 20. Name matching uses a Unicode lowercase key maintained on registration/profile changes.

| Method | Endpoint | Behavior |
| --- | --- | --- |
| GET | `/api/notes/:id/shares` or `/api/folders/:id/shares` | List direct grants as `{user: {id, name, email}, permission}` |
| PUT | `/api/notes/:id/shares/:user_id` or `/api/folders/:id/shares/:user_id` | Create/update a direct grant using `{permission: "view" \| "edit"}` |
| DELETE | `/api/notes/:id/shares/:user_id` or `/api/folders/:id/shares/:user_id` | Remove this direct grant; idempotent, 204 |

Only resource owners can list/change sharing, move, or delete items. Editors can update note contents, rename editable folders, and create notes/subfolders within editable shared folders. Such content belongs to the containing folder’s owner. Viewers cannot write. Shared resources return 403 for unauthorized operations; resources with no access return 404. Every write requires CSRF. Content updates retain revision preconditions; sharing changes do not alter content revisions.

Folder access applies dynamically to current and future descendants. Direct and inherited grants combine using the strongest permission. Removing one grant does not remove independent grants. Moving content out of an inherited shared subtree removes that inherited access; direct grants follow their resource. Deletion cascades grant rows. Access is checked from the database on every request and within write transactions, so revocation cannot be bypassed with a stale access token or client-supplied owner ID.

Note responses and folder lists include `access: "owner" | "edit" | "view"` and `owner: {id, name, email}`. Folder lists include every accessible folder, with inaccessible parents represented as null so shared subtrees can be browsed without disclosing private ancestors. A direct note share grants no access to its containing folder. All-note listing/search includes accessible owned and shared notes, without duplicates. `folder_id=shared` selects only notes owned by other users, while `folder_id=root` selects the requester’s own top-level notes. Other folder filters require access to that folder.

Sharing targets existing verified accounts. There are no public links, email invitations, or realtime presence notifications.
