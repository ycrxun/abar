# DingTalk Workbench Entry

English | [简体中文](README.zh-CN.md)

This guide defines a DingTalk Workbench link into an OIDC Agent WebUI. It does
not require DingTalk inbound messages or card callbacks.

## Required Configuration

The enterprise DingTalk administrator configures an internal application with
the public WebUI origin or an Agent-specific route as its homepage:

```text
https://abar.example.com/
https://abar.example.com/?agent=<agent-name>
```

The WebUI must be reachable over HTTPS from the DingTalk desktop client and
mobile WebView. The IdP client must include the exact WebUI callback:

```text
https://abar.example.com/auth/callback
```

The base URL opens the configured default Agent. The WebUI still supports
multiple independent Agent deployments through separate routes.

## Login Behavior

An unauthenticated Workbench entry redirects to the configured OIDC provider.
The WebUI binds the original same-origin Agent URL to the OIDC login state
and returns to it only after a successful login:

```text
DingTalk Workbench
        |
        v
/?agent=<agent-name>
        |
        v
OIDC Authorization Code + PKCE
        |
        v
/?agent=<agent-name>
```

External URLs, scheme-relative URLs, control characters, backslashes, and
oversized return paths are rejected. Different browser tabs therefore keep
their own intended Agent after login.

## Initial Enterprise Inputs

For this first step, only these values are needed:

- WebUI public HTTPS origin;
- OIDC issuer and WebUI client ID;
- exact OIDC redirect URI;
- DingTalk `CorpId`, `AgentId`, and `AppKey` for internal application
  registration;
- the test users and their OIDC identities.

`AppSecret`, callback tokens, and callback encryption keys are not required
until inbound chatbot or card callbacks are implemented.

## Local Keycloak

The local OIDC provider is Keycloak:

```bash
bash scripts/keycloak-local.sh start
```

Default connection values:

```text
Issuer: http://127.0.0.1:18093/realms/abar
Realm: abar
WebUI Client: abar-test
WebUI Redirect: http://127.0.0.1:19093/auth/callback
```

Test users:

```text
alice / alice-password      transfer-approver
bob / bob-password          viewer
charlie / charlie-password  viewer, different tenant
```

Keycloak uses `quay.io/keycloak/keycloak:26.6.1` and mounts
`tests/keycloak/keycloak-dingtalk-provider.jar` read-only at
`/opt/keycloak/providers/keycloak-dingtalk-provider.jar`. Override the JAR path
with `ABAR_KEYCLOAK_DINGTALK_PROVIDER_JAR`. Startup verifies from Keycloak logs
that `DingTalkIdentityProviderFactory` was loaded.

Start Keycloak, the ABAR Server, and an Agent WebUI:

```bash
bash scripts/workbench-local.sh start
```

Entries:

```text
http://127.0.0.1:19093/
http://127.0.0.1:19093/?agent=<agent-name>
```

These URLs simulate the target opened from DingTalk Workbench. A local loopback
URL is not reachable by a remote DingTalk client.

## Acceptance

1. Open the Workbench entry without an existing WebUI session.
2. Complete OIDC login.
3. Verify the final URL still contains `?agent=<name>`.
4. Verify the requested Agent is selected and its View Pack is displayed.
5. Repeat from desktop DingTalk and mobile DingTalk.

Current WebUI security headers reject iframe embedding. The integration
assumes DingTalk Workbench opens the URL in a top-level WebView. If the target
client embeds the WebUI in an iframe, the embedding policy must be handled
as a separate, explicitly reviewed change.

## Verification

Use the acceptance checklist above for the deployed Agent WebUI.
