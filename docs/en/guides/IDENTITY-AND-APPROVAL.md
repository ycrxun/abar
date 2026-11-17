# Identity and Approval

English | [简体中文](../../zh-CN/guides/IDENTITY-AND-APPROVAL.md)

Authentication and approval authority are separate concerns.

- `AuthProvider` proves the caller identity and tenant.
- `ApprovalAuthorizer` decides whether that identity may approve one exact
  pending Tool call.

## OIDC UserInfo mode

The bundled `abar-auth-oidc` adapter validates an access token through the
OIDC UserInfo endpoint and maps claims into `AuthContext`.

```bash
export ABAR_OIDC_USERINFO_ENDPOINT="https://id.example.com/oidc/userinfo"
export ABAR_OIDC_SUBJECT_CLAIM="sub"
export ABAR_OIDC_TENANT_CLAIM="tenant"
export ABAR_OIDC_ROLES_CLAIM="roles"
export ABAR_OIDC_APPROVER_ROLE="transfer-approver"
export ABAR_OIDC_SCOPE_FIELD="requiredScopes"
export ABAR_OIDC_ISSUER="https://id.example.com/realms/acme"
export ABAR_OIDC_AUDIENCE="abar-api"
export ABAR_OIDC_INTROSPECTION_ENDPOINT="https://id.example.com/realms/acme/protocol/openid-connect/token/introspect"
export ABAR_OIDC_CLIENT_ID="abar-server"
export ABAR_OIDC_CLIENT_SECRET="<server-side secret>"
export ABAR_OIDC_CACHE_SECONDS="30"
export ABAR_TOKEN="<OIDC access token>"
```

HTTPS is required by default. `ABAR_OIDC_ALLOW_HTTP=true` exists only for local
integration tests.

When introspection is configured, the Server requires `active=true`, exact
issuer, expected audience, and an unexpired access token before resolving
UserInfo claims. Introspection configuration is all-or-nothing: issuer,
audience, endpoint, client ID, and client secret must be supplied together.

Successful authentication results are cached by a SHA-256 hash of the token for
at most `ABAR_OIDC_CACHE_SECONDS` (default 30, maximum 300) and never beyond the
introspection expiry. Concurrent cache misses are coalesced. This trades a
bounded revocation delay for avoiding two IdP calls on every API request.

When `ABAR_OIDC_SCOPE_FIELD` is configured, an exact base role such as
`transfer-approver` remains global. Regional roles can use
`transfer-approver:华东` or `transfer-approver:华东,华南`. Visibility and
approval require every pending `requiredScopes` value to be covered by the role
suffix, and approved scopes cannot exceed that suffix.

When OIDC mode is active:

- the Server does not store the OIDC access token in `server.json`;
- CLI callers provide `ABAR_TOKEN`;
- WebUI uses Authorization Code + PKCE and keeps tokens server-side;
- approval actor comes from the UserInfo subject;
- approval roles come from the configured roles claim;
- an approver may differ from the requesting user within the same Tenant;
- an approved decision requires at least one scope.

## WebUI browser login

WebUI OIDC mode does not require a pre-issued `ABAR_TOKEN`:

```bash
export ABAR_WEBUI_OIDC_ISSUER="https://id.example.com/realms/acme"
export ABAR_WEBUI_OIDC_CLIENT_ID="abar-webui"
export ABAR_WEBUI_OIDC_REDIRECT_URI="https://abar.example.com/auth/callback"
export ABAR_WEBUI_OIDC_SCOPES="openid profile email"
```

The WebUI acts as a Backend for Frontend:

- Authorization Code + PKCE establishes the browser session;
- the browser receives only an opaque `HttpOnly`, `SameSite=Lax` cookie;
- Access and refresh tokens remain in WebUI process memory;
- WebUI sessions are process-local; restart requires users to authenticate again;
- every write requires a session-bound CSRF token;
- logout clears the WebUI session and uses the IdP end-session endpoint;
- the approval inbox lists only tenant-scoped tasks the authenticated role may
  review and omits requester Message history and execution evidence.

Set `ABAR_WEBUI_OIDC_CLIENT_SECRET` for a confidential client. HTTPS is
required unless the explicitly local-only `ABAR_WEBUI_OIDC_ALLOW_HTTP=true`
override is set.

## Local mode

Without OIDC configuration, the Server generates a local bearer token and uses
the `approver` role. This mode is for local development only.

## Security invariant

An approval cannot proceed unless:

1. the Message is waiting for approval;
2. the exact approval ID matches the pending Tool call;
3. the authenticated subject has the configured approval role;
4. the Session belongs to the same tenant;
5. an approved decision includes at least one scope.

The authorization result is recorded through the approval Runtime Event with
the actor and roles used for the decision.
