# GitHub-linked CLI account

Implementation status: the CLI integration is prepared; public browser login
requires the website endpoints in [the shared protocol](cli-account-protocol.md)
and a successful real browser round trip. Do not advertise it as deployed before
that check. v0.4.3 is the preceding command-cleanup release.

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

Local CLI provider usage remains in its existing Usage view. Account metrics are
labeled as hosted/server-recorded and do not silently include local model calls.
Unknown counts and periods are unavailable, not zero. Signing into an account
does not change the selected model, supply a provider subscription or upload
local prompts, source, conversations or usage records. Existing API-key and local
model workflows remain available.

Credentials use the existing OS store: macOS Keychain, Linux Secret Service or
Windows user-bound protection. Refresh rotates credentials under a process-shared
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
They do not establish that the website is deployed or that a live account login
has succeeded. Server replay/concurrency, workspace authorization and browser
CSRF checks belong to the website implementation and must also pass.
