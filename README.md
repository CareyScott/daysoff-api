# bv-vacation-api

Backend for BV Vacation, a friction-free vacation day tracker for small teams. Rust (Axum) running as a single serverless function on Vercel, backed by Neon Postgres.

The companion frontend (React web + Tauri desktop) lives at [bv-vacation-app](https://github.com/CareyScott/bv-vacation-app).

## How it works

- One Axum router served through the official [Vercel Rust runtime](https://vercel.com/docs/functions/runtimes/rust) (`vercel_runtime` with the `axum` feature). `vercel.json` rewrites `/api/(.*)` to the single function at `api/main.rs`.
- Postgres via sqlx (runtime queries, no macros). A shared connection pool is reused across warm invocations.
- Auth: email + password (Argon2id), 30-day JWT bearer tokens. Works identically from the browser and the Tauri desktop app.
- Absences are date ranges with a DB-level overlap exclusion constraint. Business days (Mon-Fri) are counted at booking time; vacation draws from a per-user yearly allowance, sick days are tracked but unlimited.
- Bootstrap: when the `users` table is empty, an admin account is created from the `ADMIN_EMAIL` / `ADMIN_PASSWORD` env vars.

## Local development

Requirements: Rust, Postgres (a Docker container works), [sqlx-cli](https://crates.io/crates/sqlx-cli).

```sh
cp .env.example .env         # fill in values
sqlx migrate run             # uses DATABASE_URL from .env
cargo run --bin local        # serves http://127.0.0.1:3000
curl localhost:3000/api/health
```

## Environment variables

| Var | Purpose |
|---|---|
| `DATABASE_URL` | Postgres connection string (use the pooled Neon URL in production) |
| `JWT_SECRET` | HS256 signing secret (`openssl rand -hex 32`) |
| `ADMIN_EMAIL` / `ADMIN_PASSWORD` | Initial admin account, created only when no users exist |
| `ALLOWED_ORIGINS` | Extra comma-separated CORS origins (the deployed web app URL) |

## Deployment

Deployed to Vercel (Hobby plan) with Neon Postgres from the Vercel Marketplace. Migrations run manually against the unpooled connection string:

```sh
DATABASE_URL=$DATABASE_URL_UNPOOLED sqlx migrate run
vercel deploy --prod
```

## API

All endpoints are JSON under `/api`. Authenticated routes expect `Authorization: Bearer <token>` from `POST /api/auth/login`. See `src/lib.rs` for the route table and `src/routes/` for request/response shapes.

## License

MIT
