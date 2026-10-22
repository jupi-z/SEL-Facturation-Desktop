"""Pinned SafeUpdate public keys.

The production key is intentionally not present yet.  SEL must provision the
real Ed25519 public key through an approved transition build after verifying
its fingerprint out of band.  An empty map is fail-closed: HTTPS, SHA-256, or
the ``windows_code_signed`` flag can never authorize an update by themselves.

Only public raw Ed25519 keys encoded as Base64 may be committed here.  Never
store a private key, password, test private key, or generated key material in
this file.
"""

TRUSTED_UPDATE_KEYS: dict[str, str] = {}
DEFAULT_UPDATE_CHANNEL = "stable"
