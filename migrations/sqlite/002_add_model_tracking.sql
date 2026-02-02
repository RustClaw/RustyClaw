-- Add model tracking fields to messages table
ALTER TABLE messages ADD COLUMN model_used TEXT;
ALTER TABLE messages ADD COLUMN tokens INTEGER;

-- Create index for model usage analytics
CREATE INDEX idx_messages_model ON messages(model_used);
