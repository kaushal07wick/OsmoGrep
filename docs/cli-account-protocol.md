# CLI account connection, version 1

The CLI links to the existing GitHub-backed OsmoGrep account at
`https://app.osmogrep.com`. Account login does not change model configuration,
provide a model subscription, upload local usage, or authorize execution. Existing
local models and user-supplied model keys remain usable. Browser and CLI secrets
are separate: never return GitHub access tokens, Auth.js cookies or provider keys.

The website owns these endpoints. The CLI owns the browser handoff, secure local
credentials, refresh, logout and account view. Use existing account/workspace
identity and authorization checks; a browser-selected workspace must be validated
again when its account data is read. Do not accept an arbitrary user ID from a CLI.

## Device authorization

Use RFC 8628 with public first-party client ID `osmogrep-cli` and scope
`account:read`. These endpoints accept bounded URL-encoded forms and return JSON
with `Cache-Control: no-store`. No client secret is shipped in the CLI.

- `POST /api/cli/auth/device`: fields `client_id`, `scope`. Return `device_code`,
  `user_code`, `verification_uri`, optional `verification_uri_complete`,
  `expires_in` and `interval`. The verification URLs must use the account origin.
- The browser confirmation page shows the terminal's code, the current GitHub
  account and the exact access being granted. It requires explicit confirmation
  or denial; opening a URL never approves a login. Reuse the existing GitHub
  sign-in, returning to this confirmation page without GitHub App installation.
- `POST /api/cli/auth/token`: `client_id`, `device_code`, and
  `grant_type=urn:ietf:params:oauth:grant-type:device_code`. Pending/denied/expired
  responses use the RFC error codes. Enforce polling intervals, single use and
  expiration atomically across server instances.
- The same token endpoint accepts `grant_type=refresh_token`, `client_id` and
  `refresh_token`. Return `access_token`, rotating `refresh_token`,
  `token_type=Bearer`, `expires_in` and `scope=account:read`. Invalidate the old
  refresh token atomically.
- `POST /api/cli/auth/revoke`: `client_id`, `token`,
  `token_type_hint=refresh_token`. Revoke the associated CLI session. Unknown or
  previously revoked tokens return success without revealing their status.

Use high-entropy device/access/refresh tokens and store only their hashes.
Rate-limit creation, code attempts and polling in shared storage. Bind approval
and revocation to the correct account/session; protect cookie-authenticated
approval against CSRF. Tokens must not appear in URLs or logs. Existing browser
access restrictions must also apply to CLI approval. Denial, expiration and
reused codes must never issue a token.

## Account summary

`GET /api/cli/account` accepts `Authorization: Bearer <CLI access token>` and
returns the following JSON. Reject revoked/expired tokens and re-check workspace
membership. Do not add the CLI bearer token as an alternative browser cookie or
GitHub/provider credential.

```json
{
  "user": {"id": "123", "username": "octocat", "name": "Your name"},
  "workspace": {"id": "123", "name": "Personal", "kind": "personal"},
  "stats": {"reviews": 12, "running": 0, "queued": 0, "verified": 3},
  "usage": {
    "period_start": "2026-11-01T00:00:00Z",
    "period_end": "2026-12-01T00:00:00Z",
    "model_calls": null,
    "input_tokens": null,
    "output_tokens": null
  }
}
```

User identity is required. A GitHub display name may be null; the CLI falls back
to the GitHub username. Workspace, stats and usage are optional until available.
Count fields must be nonnegative integers or null, never invented zeroes. The
stats fields match the web dashboard. Usage fields represent server-recorded
account usage for the stated period; they do not include local CLI records unless
an explicit, separately authorized synchronization feature exists. Do not return
raw jobs, private repository contents or model conversations in this summary.
The terminal labels account metrics separately from this machine's provider
usage. Missing usage, quotas, prices or plan details remain unavailable.

## Verification

Exercise pending, slow-down, success, denial, expiration, replayed code, concurrent
exchange/refresh, logout, account isolation, CSRF and revoked workspace membership.
Use a dedicated test database. Fixture protocol tests are separate from a live
browser approval; public account login is ready only after the website endpoints
and browser confirmation are deployed and the real round trip passes.
