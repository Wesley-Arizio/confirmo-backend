DROP INDEX IF EXISTS idx_credential_verifications_lookup;
DROP INDEX IF EXISTS idx_credentials_email;
DROP INDEX IF EXISTS unique_active_session_per_user;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS devices;
DROP TYPE IF EXISTS device_type;
DROP TABLE IF EXISTS credential_verifications;
DROP TABLE IF EXISTS credentials;