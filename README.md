# Task API

A small Axum + Postgres service with password login, email-code 2FA, JWT-based
auth, and admin-gated task management.

## Stack

- **Axum** — HTTP routing
- **sqlx** (Postgres) — async DB access
- **argon2** — password hashing
- **jsonwebtoken** — JWT access tokens
- **Postgres 16** — via Docker
- In-memory per-user task cache (30s TTL)

## Prerequisites

- Rust (stable, 2021 edition)
- Docker + Docker Compose
- `psql` client (optional, for running migrations by hand)

## 1. Setup

Clone the repo and create a `.env` file in the project root:

```env
DATABASE_URL=postgres://appuser:secret@localhost:5432/appdb
JWT_SECRET=replace-with-a-long-random-string
APP_ENV=development
```

`DATABASE_URL` falls back to the value above if unset. `JWT_SECRET` falls back
to an insecure dev default — **always set a real secret outside local dev**, or
tokens can be forged. `APP_ENV=development` gates the dev-only endpoint
described below.

## 2. Start Postgres (Docker)

`docker-compose.yml`:

```yaml
services:
  postgres:
    image: postgres:16-alpine
    container_name: axum_pg
    restart: unless-stopped
    environment:
      POSTGRES_USER: appuser
      POSTGRES_PASSWORD: secret
      POSTGRES_DB: appdb
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
volumes:
  pgdata:
```

Bring it up:

```bash
docker compose up -d
```

Confirm it's healthy:

```bash
docker exec axum_pg pg_isready -U appuser
```

## 3. Migrate (create schema)

Save the schema as `schema.sql`:

```sql
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    full_name       TEXT NOT NULL,
    email           TEXT NOT NULL UNIQUE,
    hashed_password TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'user',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tasks (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'todo',
    priority    TEXT NOT NULL DEFAULT 'medium',
    assigned_to UUID REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE login_challenges (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code       TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed   BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_challenges_user ON login_challenges(user_id);
```

Apply it:

```bash
docker exec -i axum_pg psql -U appuser -d appdb < schema.sql
```

## 4. Run

```bash
cargo run
# listening on 0.0.0.0:3000
```

## 5. Seed

The seed endpoint inserts demo users with **argon2-hashed** passwords. Hit it
once after migrating:

```bash
curl http://localhost:3000/seed/users
```

It returns the created users (passwords omitted). Adjust the seed list in
`src/routes/create_users.rs` to add accounts — e.g. an admin and a staff user
named "James Bond" if you want to exercise the assign-by-name flow.

## Endpoints

| Method | Path                       | Auth         | Purpose                                  |
|--------|----------------------------|--------------|------------------------------------------|
| GET    | `/seed/users`              | none         | Insert demo users (hashed passwords)     |
| POST   | `/auth/login`              | none         | Validate email/password, issue 2FA challenge |
| POST   | `/auth/verify-2fa`         | none         | Verify code, return JWT access token     |
| GET    | `/dev/email-logs/latest`   | dev only     | View the latest 2FA code (see note)      |
| POST   | `/tasks`                   | admin        | Create a task                            |
| POST   | `/tasks/assign`            | admin        | Assign tasks to a user by name           |
| GET    | `/tasks/view-my-tasks`     | any logged-in| Caller's own tasks + cache metadata      |

> **Dev endpoint note:** `/dev/email-logs/latest` is currently mounted
> unconditionally in `main.rs`. It returns the latest verification code in
> plaintext, which must never be reachable in production. Either gate the route
> behind a `#[cfg(feature = "dev")]` block and a runtime `APP_ENV=="development"`
> check, or remove it before deploying. As written, the handler should refuse
> unless `APP_ENV=development`.

## 6. Validation rules

- **Login** — `email` must contain `@` and be at least 3 chars; `password`
  must be non-empty. Unknown email and wrong password both return the same
  `401 invalid credentials` to prevent account enumeration.
- **2FA verify** — challenge must exist, be unconsumed, and be within its
  5-minute expiry; the submitted code must match. Codes are single-use
  (`consumed` flag flips on success).
- **JWT** — `Authorization: Bearer <token>` required on protected routes.
  Tokens carry `sub` (user id) and `role`, expire after 1 hour, and are
  rejected if expired or signature-invalid.
- **Create task** — `title` must be non-empty; if `assigned_to` is supplied it
  must reference an existing user (returns `400` otherwise).
- **Assign tasks** — `task_ids` non-empty; `assignee_name` resolved against
  `users.full_name` (returns `404` if no match, `409` if the name is ambiguous;
  non-existent task ids are skipped and reported via `updated_count`).

## 7. Test (manual end-to-end)

```bash
# 1. seed
curl http://localhost:3000/seed/users

# 2. login -> prints 2FA code to the SERVER CONSOLE, returns challenge_id
curl -X POST http://localhost:3000/auth/login \
  -H 'content-type: application/json' \
  -d '{"email":"alice@example.com","password":"password123"}'

# read the code from the server console, or in dev:
curl http://localhost:3000/dev/email-logs/latest

# 3. verify -> returns { access_token, token_type, expires_in }
curl -X POST http://localhost:3000/auth/verify-2fa \
  -H 'content-type: application/json' \
  -d '{"challenge_id":"<id>","code":"<code>"}'

export TOKEN="eyJ..."

# 4. create a task (admin token required)
curl -X POST http://localhost:3000/tasks \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"title":"Ship the release","priority":"high"}'

# 5. assign tasks to a user by name (admin)
curl -X POST http://localhost:3000/tasks/assign \
  -H 'content-type: application/json' \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"task_ids":["<uuid1>","<uuid2>"],"assignee_name":"James Bond"}'

# 6. view your own tasks (any authenticated user)
curl http://localhost:3000/tasks/view-my-tasks \
  -H "Authorization: Bearer $TOKEN"
# first call -> "cache": { "hit": false }; repeat within 30s -> "hit": true
```

### Expected auth failures

```bash
# no token
curl -i http://localhost:3000/tasks/view-my-tasks            # 401

# non-admin hitting an admin route
curl -i -X POST http://localhost:3000/tasks \
  -H "Authorization: Bearer $STAFF_TOKEN" -d '{"title":"x"}'  # 403
```

## Notes & limitations

- **2FA codes** are logged to the console in place of email and stored in
  plaintext in the DB — fine for this exercise, not for production (hash them
  and rate-limit verification).
- **No rate limiting** on login or 2FA verify; a 6-digit code is brute-forceable.
- **JWT role is baked in at issue time**, so a demoted admin keeps admin access
  until their token expires (1h). Use short expiry or per-request DB checks for
  sensitive routes if you need instant revocation.
- **Task cache** is per-process and not invalidated on write, so a freshly
  assigned task may not appear in `view-my-tasks` for up to 30s.
- **Assign-by-name** depends on `full_name`, which is not unique; prefer
  assigning by id or add a unique constraint if names collide.

  