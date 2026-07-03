# daysoff-api

Backend for **Daysoff**, an open-source, self-hosted vacation day tracker for small teams. Rust (Axum) running as a single serverless function on Vercel, backed by your own Neon Postgres database. Free-tier friendly: a small team runs at $0/month.

The companion frontend (React web + Tauri desktop) lives at [daysoff-app](https://github.com/CareyScott/daysoff-app).

## Features

- Email + password accounts with admin and member roles
- Direct booking, no approval workflow: pick a date range, done
- Vacation draws from a per-user yearly allowance; sick days tracked separately (unlimited)
- Business-day counting (weekends excluded automatically), DB-level overlap prevention
- White-label: company name and accent color are stored in your database and editable in-app by admins
- Your data stays in your own database

## Deploy your own

Requirements: a GitHub account, a free Vercel account, Rust locally for migrations.

1. Fork/clone this repo, then create a Vercel project from it (framework: Other).
2. Add Neon Postgres: Vercel dashboard → your project → Storage → Create Database → Neon (free tier). This injects `DATABASE_URL` and `DATABASE_URL_UNPOOLED`.
3. Set the remaining environment variables (see below).
4. Run migrations from your machine:
   ```sh
   cargo install sqlx-cli --no-default-features --features rustls,postgres
   DATABASE_URL=<your DATABASE_URL_UNPOOLED> sqlx migrate run
   ```
5. Deploy (`vercel deploy --prod` or git push with the GitHub integration).
6. First request bootstraps the admin account from `ADMIN_EMAIL` / `ADMIN_PASSWORD`. Log in, change the password, set your company name and color in Settings, invite your team.

## Environment variables

| Var | Purpose |
|---|---|
| `DATABASE_URL` | Postgres connection string (pooled Neon URL; injected by the integration) |
| `JWT_SECRET` | HS256 signing secret (`openssl rand -hex 32`) |
| `ADMIN_EMAIL` / `ADMIN_PASSWORD` | Initial admin account, created only when no users exist |
| `ADMIN_NAME` | Display name for the initial admin (optional, default "Admin") |
| `ALLOWED_ORIGINS` | Extra comma-separated CORS origins (your deployed web app URL) |

## Local development

```sh
cp .env.example .env         # fill in values; a local Postgres or Docker container works
sqlx migrate run
cargo run --bin local        # serves http://127.0.0.1:3000
curl localhost:3000/api/health
```

## How it works

- One Axum router served through the official [Vercel Rust runtime](https://vercel.com/docs/functions/runtimes/rust) (`vercel_runtime` with the `axum` feature). `vercel.json` rewrites `/api/(.*)` to the single function at `api/main.rs`.
- Postgres via sqlx (runtime queries, no macros). A shared connection pool is reused across warm invocations.
- Auth: Argon2id password hashing, 30-day JWT bearer tokens. Works identically from the browser and the Tauri desktop app.
- Absences are date ranges with a DB-level overlap exclusion constraint.

## API

All endpoints are JSON under `/api`. Authenticated routes expect `Authorization: Bearer <token>` from `POST /api/auth/login`. See `src/lib.rs` for the route table and `src/routes/` for request/response shapes.

## License

MIT
