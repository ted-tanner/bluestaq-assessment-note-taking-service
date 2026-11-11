# Bluestaq Notes

## Contents

- [Intro](#intro)
- [Backend design choices and compromises](#backend-design-choices-and-compromises)
- [Frontend design choices and compromises](#frontend-design-choices-and-compromises)
- [Future functionality](#future-functionality)
- [What I'd change about how I approached this](#what-id-change-about-how-i-approached-this)
- [How to run](#how-to-run)
  - [Backend](#backend)
  - [Frontend](#frontend)

## Intro

Cool technical assessment! I appreciate you being willing to consider me for this role.

The app works and the follows good security practices. Follow the [How to run](#how-to-run) instructions if you want to see it for yourself. I tried to build something useful, but simple.

I used AI (Codex) extensively to help me build this in the short time allotted, but I was very present and engaged with the code, especially the backend. I drove ang gave specific requirements and feedback to the agents. I spent most of my time on the backend because that is where the security boundaries and data-safety boundaries primarily reside. To be totally transparent, the frontend is a lot less refined than the backend. If I had more time, I'd love to build a frontend that felt like mine rather than just AI slop that I prompted into a state that isn't terrible.

## Backend design choices and compromises

The challenge was described as a note-taking service "used and shared amongst several small teams." Given that small scope, I chose to keep things simple and store notes into a relational database. For ease of running the prototype, I used sqlite, though I'd usually recommend Postgres so the application server can scale horizontally behind a load balancer, if needed. If the scale were to grow, I'd consider not storing all of the notes in the DB (DB storage is expensive!) and move them over to blob storage. However, best to keep things simple until something less simple is needed.

I'm very experienced building authentication/authorization systems and am aware of the security pitfalls, so it seemed quicker and easier to me to build out a simple, custom JWT scheme than to use an existing service. This works well for this app and scope, but often auth has more requirements than this app does (e.g. granular permission sets, federated auth). In that case, an off-the-shelf solution may be the right way to go if the team doesn't want the burden of maintaining auth.

## Frontend design choices and compromises

The frontend needs some loving. Whereas I scrutinized backend code, I essentially just vibe coded the front end. There are still some rough edges in the UI that I didn't get the time to polish and I would provide better organization for the code if I had more time.

## Future functionality

I thought through some functionality that I could add to make this a better product. A lot of them never got implemented, unfortunately. Here is my to-do list:

- Frontent polish (the frontend feels AI generated because it mostly is)
- "Recently deleted" for notes
- Use websockets to allow simultaneous editting of a note and ability to view where another person's cursor is
- Tags for notes
- Groups (such that a note or folder may be shared with everyone in a group simultaneously)
- Export notes as markdown or PDF
- Improved search
- Improved editor
- Administrative functionality
  - Admins may gate who can join and create notes
  - Access management for groups and individuals

## What I'd change about how I approached this

I didn't leave enough time for a final validation/polish phase. I would love to provide a more polished prototype, so I should have given myself more time for that final phase. I should have been a little less particular with making the backend perfect and devoted more time to the frontend.

## How to run

Prerequisites: Rust/Cargo, Node.js, and pnpm.

### Backend

From the repository root:

```sh
cd backend
cp sample.env .env
```

Generate two separate keys by running this command twice, then set `TOKEN_SIGNING_KEY_B64` and `PASSWORD_PEPPER_B64` in `.env` to the respective values. Keep `SECURE_COOKIES=false` for local HTTP development.

```sh
dd if=/dev/urandom bs=32 count=1 2>/dev/null | base64
```

Start the API:

```sh
cargo run
```

The API runs at `http://127.0.0.1:8080`. SQLite and migrations are handled automatically. Email verification codes appear in the backend terminal.

### Frontend

In a separate terminal, from the repository root:

```sh
cd frontend
pnpm install
pnpm dev
```

Open the local URL printed by Vite (usually `http://localhost:5173`). The frontend proxies `/api` requests to the running backend.
