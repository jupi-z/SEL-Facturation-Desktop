-- Keep the session across app restarts, but cap all sessions at three days from sign-in.
UPDATE auth_sessions
SET refresh_expires_at = LEAST(refresh_expires_at, issued_at + INTERVAL '3 days')
WHERE revoked_at IS NULL;
