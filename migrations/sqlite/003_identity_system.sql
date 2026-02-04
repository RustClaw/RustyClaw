-- Migration: 003_identity_system
-- Description: Adds users, identities, and linking tables for Unified Identity

-- 1. Users Table (The Humans)
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT UNIQUE NOT NULL,
    role TEXT NOT NULL DEFAULT 'user', -- 'admin', 'user'
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- 2. Identities Table (The Access Methods)
-- Links a platform ID (Telegram ID, API Key hash, etc.) to a User
CREATE TABLE IF NOT EXISTS identities (
    provider TEXT NOT NULL, -- 'web', 'telegram', 'whatsapp', 'api_token'
    provider_id TEXT NOT NULL, -- Platform specific ID
    user_id TEXT NOT NULL, -- FK to users
    label TEXT, -- User-friendly name ('Chrome on Laptop', 'My iPhone')
    created_at DATETIME NOT NULL,
    last_used_at DATETIME,
    metadata TEXT, -- JSON blob for extra provider info
    PRIMARY KEY (provider, provider_id),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- 3. Pending Links (For secure channel linking)
-- Used when a Web user wants to attach their Telegram/WhatsApp
CREATE TABLE IF NOT EXISTS pending_links (
    code TEXT PRIMARY KEY NOT NULL, -- The OTP shown on web
    user_id TEXT NOT NULL, -- The user claiming the channel
    provider TEXT NOT NULL, -- The expected provider (e.g., 'telegram')
    expires_at DATETIME NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_identities_user_id ON identities(user_id);
