# AI Usage
 
This document discloses where AI tooling was used in building this project and
what was written or changed manually.
 
## Tools used
 
- **Claude (Anthropic)** — used interactively to scaffold endpoints, draft SQL
  schema, and generate boilerplate (Axum handlers, sqlx queries, JWT helpers,
  argon2 password hashing) and to write the project README and this file.
## What AI generated
 
- Initial Docker Compose and Postgres setup.
- Schema for `users`, `tasks`, and `login_challenges`.
- Endpoint scaffolding: seed users, `/auth/login`, `/auth/verify-2fa`,
  dev verification-code viewer, task create/assign, and `view-my-tasks`.
- Password hashing/verification helpers (argon2) and JWT issue/decode helpers.
- The in-memory per-user task cache structure and TTL logic.
- Documentation (`README.md`, this file).
