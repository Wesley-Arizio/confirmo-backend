DROP INDEX IF EXISTS idx_conversation_participants_user;
DROP INDEX IF EXISTS idx_messages_conversation;
DROP INDEX IF EXISTS idx_unique_participant_per_conversation;
DROP INDEX IF EXISTS idx_one_pending_submission_per_user;
DROP TABLE IF EXISTS submissions;
DROP TYPE IF EXISTS submission_status;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS conversation_participants;
DROP TABLE IF EXISTS conversations;
DROP TABLE IF EXISTS clients;
DROP TABLE IF EXISTS lawyers;
DROP TABLE IF EXISTS users;
DROP TYPE IF EXISTS conversation_type;
DROP TYPE IF EXISTS role;
DROP TYPE IF EXISTS status;

-- Add down migration script here
