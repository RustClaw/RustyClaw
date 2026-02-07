-- Add password_hash column to users table
ALTER TABLE users ADD COLUMN password_hash TEXT;

-- Create index for faster username lookups
CREATE INDEX idx_users_username ON users(username);
