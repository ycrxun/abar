# Platform Console

English | [简体中文](README.zh-CN.md)

Platform Console is the management interface for the ABAR control plane. It is
separate from Agent WebUI:

- Agent WebUI: business users converse with one Agent and use business views;
- Platform Console: operators manage the Agent catalog, Revisions, Scope Review,
  and Runtime state.

Platform Console does not expose business Session conversations, prompts, full
Tool arguments, or final business output.

## Capabilities

Platform Console provides:

- the current Tenant's Agent catalog;
- Current and historical Revisions for each Agent;
- model class, maximum steps, Tool, Skill, and Knowledge scopes;
- Scope Review evidence with approve/reject actions;
- aggregate active workers, Sessions, Messages, Tokens, Provider, and Storage;
- OIDC BFF login and CSRF protection.

The UI presents facts persisted by the Server.

## Authority

The Server uses `ABAR_CONTROL_PLANE_ROLE` as the platform administrative role,
defaulting to `platform-admin`. A local Server grants both `approver` and
`platform-admin`.

Platform Scope Review and business write approval are separate authorities:

- Scope Review decides whether a candidate Revision becomes Current;
- business approval decides whether one exact Tool call executes.

The local workbench assigns an approver role through its local identity fixture.
Production should use a dedicated IdP role for Platform Console operations.

## API

```text
GET  /v1/control-plane/overview
POST /v1/control-plane/agents/{agent}/revisions/{revision}/review
```

`overview` returns Tenant aggregates without per-Session detail. Review accepts
only `approved` or `rejected`; the reviewer is the authenticated Subject.

## Start

```bash
ABAR_PLATFORM_OIDC_ISSUER=https://id.example.com/realms/acme \
ABAR_PLATFORM_OIDC_CLIENT_ID=abar-platform \
ABAR_PLATFORM_OIDC_REDIRECT_URI=https://platform.example.com/auth/callback \
ABAR_PLATFORM_OIDC_SCOPES="openid profile email" \
abar-platform-console --addr 127.0.0.1:19092
```

The local workbench starts Agent WebUI (`19093`) and Platform Console (`19094`)
together.

## Verification

```bash
bash scripts/test-platform-console.sh
```

The test covers first publication, a Scope expansion candidate, approval from
Platform Console, Current Revision activation, and Runtime/Token aggregation.
