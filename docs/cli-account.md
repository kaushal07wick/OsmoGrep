# GitHub-linked CLI account

Available in v0.4.7. The client uses the deployed account endpoints in
[the shared protocol](cli-account-protocol.md). Existing dashboard accounts sign
in through the browser; no GitHub App installation is required.

Inside the TUI, **`/login`** opens Account and starts browser authorization. The
verification code and waiting/cancel state stay in the TUI. No model call is used.
Shell commands are optional alternatives:

```sh
osmogrep login
osmogrep account
osmogrep account --json
osmogrep logout
```

Login opens the account confirmation in the browser, shows a short code and waits
for approval. The user signs into the existing GitHub-backed dashboard account,
checks the code and explicitly approves the CLI connection. The website must not
require GitHub App repository installation for account sign-in.

For a remote shell, `osmogrep login --no-browser` prints the link and code without
opening a browser. Ctrl+C cancels. A fresh, unconfigured terminal opens a local welcome view with **Sign in with
browser** and **Choose a local model or API key**. It makes no account request
until the user selects an action. Existing installations go straight to chat.
The terminal also exposes **Settings → Account** for sign-in, current profile, workspace, server-recorded review stats and usage.
Closing that view cancels its pending operation. The ordinary chat startup makes
no account request. It adds only the explicit `/login` control, with no account context in model prompts.

The Account panel uses a portrait layout with recorded workspace counts, an
animated dithered activity chart and a cache ring. **This Mac** shows the current
UTC month's provider-reported usage across local workspaces; **Hosted** shows only
the account server's figures. Cached input is included in input, not added to it.
The top three models have compact usage bars; **All models** expands the rest.
Use Tab / Left / Right and Enter for actions, and arrows or the mouse wheel to
scroll a smaller terminal. Reduced motion shows settled charts immediately.
The existing workspace-scoped Usage view keeps its original range and scope.

A successful account lookup caches presentation-only identity and a small circular
GitHub portrait in `account/profile.json`. Startup reads this local cache without
an account HTTP request. Portrait downloads are bounded and optional; text-only
terminals use initials. Logout and account switching clear the identity cache.
The welcome header keeps the OsmoGrep mark, wordmark, account name and repo path,
and hides from the second user message.
Unknown counts and periods are unavailable, not zero. Signing into an account
does not change the selected model, supply a provider subscription or upload
local prompts, source, conversations or usage records. Existing API-key and local
model workflows remain available.

Account login is saved in `account/login.json` inside the application configuration
directory. The directory is private (0700) and the token file is readable and writable
only by your OS user (0600) on macOS/Linux. Windows reuses the existing local DPAPI-encrypted credential
file, which only the current Windows user can decrypt. The Unix account file
is not encrypted at rest. Provider API keys and provider OAuth
credentials retain their existing OS credential storage. Refresh rotates credentials under a process-shared
lock also used by login and logout. A failed remote revocation still removes the
local credential and reports that remote revocation was not confirmed. A failure
to remove the local credential remains an error. Profile responses are bounded,
validated and never followed across HTTP redirects.

On this machine browser links use the existing Firefox Personal launcher. Other
installations retain the configured `OSMOGREP_BROWSER` or platform opener.
`OSMOGREP_ACCOUNT_URL` supports a separately deployed account service using an
HTTPS origin; HTTP is allowed only for loopback development. It must not contain
a path, credentials, query or fragment. No provider client secret is bundled.

The existing registered-provider flow remains available as
`osmogrep auth login PROVIDER`; it continues to use the user's registered OAuth
configuration. Omitting the provider selects the first-party OsmoGrep account.

Local fixtures verify the client protocol, response limits, invalid metrics,
origin/redirect restrictions, waiting/cancellation, slowdown, denial and expiry.
The release fixture also checks private-file persistence across processes,
forced refresh, rotated-token storage, remote revocation and local logout.
The Mac account implementation completed real browser approval, private-file persistence,
account lookup, forced Rust refresh, rotated-token storage and logout against the
deployed service. Both access-token generations were rejected after revocation.
The disposable test connection was revoked and its local file removed. That live
run used the v0.4.4 candidate; v0.4.7 retains the Unix login logic, reuses the existing
Windows DPAPI file with isolated PowerShell module discovery, and passes the release
fixtures again. Model-provider lookups explicitly exclude the account token. See
[the validation record](cli-account-validation.json) for these distinct scopes.
