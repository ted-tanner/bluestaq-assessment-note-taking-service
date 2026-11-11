# Bluestaq Notes API

One Cargo package with an Actix Web API. SQLite is compiled into the application, so no system SQLite installation is needed. Diesel manages the database; migrations run automatically at startup. The frontend is not connected yet.

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
3. Register with email/password. Registration returns `202` with `{challenge_id, purpose: "registration", expires_in: 600}`; it does not issue an access token yet.
4. Read the code in the mock email output. Submit `{challenge_id, code}` to `/api/auth/register/verify`. Success sets `access_token` and `refresh_token` cookies and returns `{user: {id, email}}`.
5. Later sign-ins return a challenge with purpose `signin`. Submit it and the emailed code to `/api/auth/2fa/email/verify` before accessing notes. Sign-in to a pending account instead returns a registration challenge.

Both auth cookies are host-only, HTTP-only, SameSite Strict, and scoped to `/`. `AuthenticatedUser` verifies access tokens locally without a database lookup. Access tokens default to five minutes (`ACCESS_TOKEN_LIFETIME_SECONDS=300`); refresh tokens default to seventy-two hours (`REFRESH_TOKEN_LIFETIME_SECONDS=259200`). Both settings must be positive integers. Normal requests do not renew access tokens.

Call `POST /auth/refresh` with the refresh cookie and CSRF header to obtain a new pair, including after the access token expires. The old refresh token is atomically blacklisted and cannot be used twice; serialize refresh calls in the client. Tokens have distinct types, and refresh tokens contain random nonces so rotations within the same second still produce different tokens. Both lifetimes remain capped by the original thirty-day login deadline; refreshing never extends that deadline. If the refresh token expires, sign in with password and email code again.

Only the CSRF cookie is readable by JavaScript. It is retained across refreshes and cleared on logout or password reset. Fetch `/auth/csrf-token` again if it is missing. Logout and password reset blacklist the refresh token presented by this browser and clear all three cookies. Copied access tokens remain usable until their short expiration. Password reset increments the user’s database `token_version`, invalidating every previously issued refresh token. Logout revokes only the presented refresh token. Signing-key rotation invalidates all tokens. There is no session table; `/user/me` uses the identity snapshot in the signed access token. Refresh requests compare the token’s signed version with the current user row, including a recheck inside the blacklist transaction. The version persists across restarts and works across instances sharing the database. Access validation does not query it. All API instances must use the same signing key and database.

The refresh blacklist stores token fingerprints and their expiration times. Each insertion checks a shared in-memory `last_blacklist_clear_time`, initialized at process startup. Once more than six hours have passed since the last successful cleanup, count rows whose expiry is strictly earlier than `now - refresh-token lifetime`. Delete those rows only when the count is greater than 100, then advance the timestamp after commit. Skipped checks and failed transactions do not advance it. No job runner or persisted cleanup timestamp is needed; restarting merely delays the next check.

Email codes are eight case-insensitive alphanumeric characters, expire after ten minutes, and permit five attempts. Resend requires sixty seconds, replaces the challenge identifier, and invalidates the previous code. A challenge can be used only for its specified purpose and only once. Codes are hashed in the database and never returned by the API.

Reset requests return the same `202` response shape whether or not an account exists (with an opaque placeholder challenge when no email is sent or the request is on cooldown). Submit `{challenge_id, code, new_password}` to confirm a reset. It invalidates outstanding email challenges, clears this browser’s cookies, and requires signing in again in that browser. The database token version is incremented atomically with the password update, so all old refresh tokens are rejected, including those from other browsers. The presented refresh token is also blacklisted. Resetting a password does not issue an access token or bypass sign-in 2FA.

Password and email-code hashing/verification run on Rayon’s CPU pool; Diesel connection checkout, queries, and transactions run through `web::block`. Database connections are released before awaiting expensive crypto. At most `HASH_CONCURRENCY` Argon2 operations run simultaneously per service instance (default 2); excess requests receive `429`.

## Endpoints

All paths are prefixed with `/api`. Requests and responses use JSON. Application errors use `{"error":"message"}`; malformed requests and unmatched routes use Actix’s default error responses. Verification failures return `401`, CSRF failures `422`, inaccessible notes `404`, and limits `429`.

| Method | Path | Request / behavior |
| --- | --- | --- |
| GET | `/health` | Process liveness |
| GET | `/auth/csrf-token` | Set CSRF cookie |
| POST | `/auth/register` | `{email, password}`; pending account, `202` |
| POST | `/auth/register/verify` | `{challenge_id, code}`; access cookie, `200` |
| POST | `/auth/sign-in` | `{email, password}`; email challenge, `202` |
| POST | `/auth/2fa/email/verify` | `{challenge_id, code}`; access cookie, `200` |
| POST | `/auth/email/resend` | `{challenge_id}`; replacement challenge, `202` |
| POST | `/auth/refresh` | Refresh cookie; rotate tokens, `200` |
| POST | `/auth/logout` | Blacklist presented refresh token and clear cookies, `204` |
| POST | `/auth/password-reset/request` | `{email}`; generic challenge response, `202` |
| POST | `/auth/password-reset/confirm` | `{challenge_id, code, new_password}`; `204` |
| GET | `/user/me` | Current `{id, email}` |
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
  -d '{"email":"you@example.com","password":"a long example password"}' \
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
