CREATE TYPE status as ENUM ('pending_email_verification', 'email_verified', 'active', 'suspended', 'disabled');

CREATE TYPE role as ENUM ('lawyer', 'client', 'admin');

CREATE TYPE conversation_type as ENUM ('direct', 'case', 'organization', 'system');

-- Add up migration script here
CREATE TABLE users (
    id UUID NOT NULL PRIMARY KEY,
    email VARCHAR NOT NULL UNIQUE,
    name VARCHAR NOT NULL,
    status status NOT NULL,
    role role NOT NULL,
    email_verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ
);

CREATE TABLE lawyers (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    oab_number VARCHAR UNIQUE NOT NULL
);

CREATE TABLE clients (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE conversations (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_type conversation_type NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    owner_user_id UUID NOT NULL REFERENCES users (id)
);

CREATE TABLE conversation_participants (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    joined_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    conversation_id UUID NOT NULL REFERENCES conversations (id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE messages (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    cipher_text VARCHAR NOT NULL,
    encryption_version VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    sender_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    conversation_id UUID NOT NULL REFERENCES conversations (id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE UNIQUE INDEX idx_unique_participant_per_conversation ON conversation_participants (conversation_id, user_id);
CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);
CREATE INDEX idx_conversation_participants_user ON conversation_participants (user_id);