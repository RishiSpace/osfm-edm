-- Hash leftover plaintext enrollment tokens (UUIDs) so the API can look up
-- SHA-256 hex only. 64-char values are assumed already hashed.
UPDATE enrollment_tokens
SET token = encode(digest(token, 'sha256'), 'hex')
WHERE length(token) <> 64;
