# Bluestaq Notes

- `backend/`: standalone Rust binary package (no Cargo workspace).
- `frontend/`: SvelteKit application with Svelte, TypeScript, and Vite.

## Backend

Install Rust with Cargo, then run:

```sh
cd backend
cargo run
```

The backend currently prints a welcome message. To check it:

```sh
cargo check
cargo fmt --check
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
