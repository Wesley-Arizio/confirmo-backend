CREATE TYPE status as ENUM ('pending_email_verification', 'email_verified', 'active', 'suspended', 'disabled');

CREATE TYPE role as ENUM ('lawyer', 'client', 'admin');

CREATE TYPE conversation_type as ENUM ('direct', 'case', 'organization', 'system');


CREATE TABLE firms (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ
);

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
    oab_number VARCHAR UNIQUE NOT NULL,
    firm_id UUID NOT NULL REFERENCES firms (id) ON DELETE RESTRICT
);

CREATE TABLE clients (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE conversations (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_type conversation_type NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    owner_user_id UUID NOT NULL REFERENCES users (id),
    firm_id UUID NOT NULL REFERENCES firms (id) ON DELETE RESTRICT,
    CONSTRAINT conversations_id_firm_id_key UNIQUE (id, firm_id)
);

CREATE TABLE conversation_participants (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    joined_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    conversation_id UUID NOT NULL,
    firm_id UUID NOT NULL,
    CONSTRAINT conversation_participants_conversation_fkey
        FOREIGN KEY (conversation_id, firm_id) REFERENCES conversations (id, firm_id)
        ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE messages (
    id UUID NOT NULL PRIMARY KEY DEFAULT gen_random_uuid(),
    cipher_text VARCHAR NOT NULL,
    encryption_version VARCHAR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ,
    sender_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE ON UPDATE CASCADE,
    conversation_id UUID NOT NULL,
    firm_id UUID NOT NULL,
    CONSTRAINT messages_conversation_fkey
        FOREIGN KEY (conversation_id, firm_id) REFERENCES conversations (id, firm_id)
        ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE UNIQUE INDEX idx_unique_participant_per_conversation ON conversation_participants (conversation_id, user_id);
CREATE INDEX idx_messages_conversation ON messages (conversation_id, created_at);
CREATE INDEX idx_conversation_participants_user ON conversation_participants (user_id);

CREATE INDEX idx_lawyers_firm ON lawyers (firm_id);
CREATE INDEX idx_conversations_firm ON conversations (firm_id, created_at);
CREATE INDEX idx_conversation_participants_firm ON conversation_participants (firm_id);
CREATE INDEX idx_messages_firm ON messages (firm_id, created_at);
