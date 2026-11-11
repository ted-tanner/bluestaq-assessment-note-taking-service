# Bluestaq Notes

- `backend/`: standalone Rust binary package (no Cargo workspace).
- `frontend/`: SvelteKit application with Svelte, TypeScript, and Vite.

## Backend

See [backend/README.md](backend/README.md) for environment setup, authentication flows, and API examples. After configuring `backend/.env`, run:

```sh
cd backend
cargo run
```

To check the backend:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Frontend

Install Node.js and pnpm, then run:

```sh
cd frontend
pnpm install
pnpm dev
```

Open the local URL printed by Vite. To check and build the frontend:

```sh
pnpm check
pnpm build
```
