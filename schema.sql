CREATE TABLE login_challenges (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code       TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed   BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_challenges_user ON login_challenges(user_id);