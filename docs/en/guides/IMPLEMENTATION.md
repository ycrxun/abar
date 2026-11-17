# Implementation

English | [简体中文](../../zh-CN/guides/IMPLEMENTATION.md)

This is the engineering implementation guide. Business authors should start
with [Define a Business Agent](AUTHORING.md).

## Workspace

```text
abar-api/        stable contracts, JSON Schema, OpenAPI, validators
abar-core/       Revision Build, Session/Message stores, workers, providers
abar-cli/        abar command-line client
adapters/        reference Model and Tool providers
apps/abar-webui/
apps/abar-platform-console/
examples/merchandise-operations/
views/           declarative WebUI View Packs
```

## Agent Revision Build

`abar publish`:

1. reads Revision project YAML;
2. validates source paths and rejects symlinks;
3. computes Prompt, Skill, rule, Knowledge, and scope digests;
4. computes a conservative Scope Diff;
5. stores an immutable Revision;
6. moves the current pointer directly for behavior-only changes;
7. stores Scope expansion as `waiting_review`.

`abar revision approve` stores Review evidence and moves the current pointer only
when evidence is bound to the current base Revision.

## Session and Message storage

Default state:

```text
<state-dir>/
├── revisions/
└── sessions/
    ├── approval-index.json
    └── <session-id>/
        ├── session.json
        └── messages/
            ├── <message-id>.json
            └── <message-id>.events.jsonl
```

Writes are atomic, Session mutations are file-locked, and Events are fsynced
before success.

## Message execution

1. Resolve current Agent Revision.
2. Resolve Provider Bundle and persist Execution Composition.
3. Create `queued` Message.
4. Dispatch background worker.
5. Build Context from Prompt, Skills, Knowledge, history, projected Tool
   results, and current input.
6. Execute Model/Tool iterations.
7. Persist approval, external wait, output, Tool results, and Events.

Cancellation is checked before model and Tool calls and again after model
responses.

## Approval and recovery

Pending write evidence stores Tool ID, exact arguments, idempotency key, and
approval ID. Approval cannot cross Message or Revision.

Restart behavior:

| State | Recovery |
|---|---|
| `queued` | Dispatch worker |
| `running` | Resume checkpoint or approved write |
| `waiting_approval` | No automatic execution |
| `waiting_external` | Query exact persisted Tool call |

The bundled MCP adapter invokes a configured read-only
`idempotencyCheckTool`. A missing write result remains unconfirmed.

## Context and sensitive data

`ContextAssembler` includes completed history only and never injects failed or
cancelled output. Tool results pass through `DataPolicy` before storage in
model context. Denied fields are removed and sensitive scalar values are guarded
from model output.

## WebUI

`abar-webui --agent <name>` uses only `/v1` APIs and supports:

- Session list;
- Message history within one Session;
- multi-turn replies within the selected Session;
- explicit “confirm execution” that appends a Message and can produce a write
  intent;
- Tenant and role-scoped approval inbox with minimal evidence;
- Composition, pending approval, Tool results, output, and Runtime Events;
- approve/reject;
- cancel and external reconciliation.

The WebUI is conversation-first. A user can start a Session with an
arbitrary message and continue through unrestricted multi-turn dialogue. View
Pack task actions are optional prompt shortcuts, not the only supported path.
Write intent still passes through Tool Scope, exact approval, and idempotent
external execution.

The WebUI binds one Agent and one View Pack at startup. `GET /api/view`
returns the View Pack; Session and approval queries are filtered to the bound
Agent. Server-side multi-Agent support remains available without carrying
unused Agent-selection state in the WebUI.

Session lists carry the latest Message state from the Server, avoiding per-row
Message requests. `POST /v1/tasks` creates the Session and first Message as one
idempotent request keyed by `requestId`; failed preparation archives the
placeholder Session. WebUI confirmation carries an opaque business confirmation
object plus `agentRevision`, and validates that exact object against the
selected Message's Tool evidence before creating a Message.

The BFF presents structured confirmation Messages as business text in Session
titles and conversation turns. Internal protocol markers and business-specific
confirmation fields remain in the persisted Message and are not rendered to
users.

Message Events use a cursor. Polling requests only Events after
`eventCursor`; conversation rendering preserves the user's scroll position and
stale detail responses are discarded. The browser UI is split into HTML, CSS,
and an ES Module, allowing `script-src 'self'` and `style-src 'self'` without
`unsafe-inline`.

In OIDC mode, WebUI uses Authorization Code + PKCE and acts as a Backend for
Frontend. The browser receives only an opaque `HttpOnly`, `SameSite=Lax` session
cookie. Access and refresh tokens remain in WebUI memory. Writes require a
session-bound CSRF token. The browser never receives an ABAR Server bearer
token or IdP access token.

WebUI polling pauses while the page is hidden and resumes with one serialized
refresh when visible.

The MCP adapter forwards `x-abar-session-id` and `x-abar-tenant-id`. Business
Tools may therefore pin one data snapshot across multiple Messages. The example
Mock MCP returns its own confirmation fields and advances a snapshot only for
`refreshSnapshot=true`.

## Platform Console

`abar-platform-console` consumes `/v1/control-plane/*` through an OIDC BFF. The
Server requires `ABAR_CONTROL_PLANE_ROLE` before returning any control-plane
data. Platform Console exposes Agent/Revision inventory, Scope Review, and
Tenant-scoped Runtime aggregates. It never receives Session conversations,
prompts, full Tool arguments, or final business output.

Scope Review uses the Subject ID as reviewer evidence and activates an approved
candidate Revision as Current. Business write approval remains a separate
authority handled by Agent WebUI.

## Identity and approval

Local mode uses a generated bearer token, the `approver` role, and an exact
approval scope. OIDC mode uses optional strict token introspection followed by
UserInfo. Introspection verifies active state, issuer, audience, and expiry.
`server.json` never stores the OIDC access token.

Verified identity is cached by token hash for a bounded TTL
(`ABAR_OIDC_CACHE_SECONDS`, default 30, maximum 300). Concurrent cache misses
are coalesced, and cache lifetime never exceeds the token expiry returned by
introspection.

CLI callers provide `ABAR_TOKEN`. WebUI users authenticate through the browser;
WebUI obtains and refreshes tokens server-side.

`GET /v1/approvals` returns only pending tasks the authenticated reviewer may
handle within the same Tenant. The projection contains approval coordinates,
Tool ID, exact arguments, Agent Revision, requester, and pending time. It omits
requester history, final output, composition, and Tool-result evidence.

The inbox reads a durable `approval-index.json` under the Session store.
Approval request, cancellation, completion, failure, and decision paths update
the index with state writes. Server startup rebuilds it from Message source
records, and `abar state reindex-approvals` provides an explicit offline rebuild.

The approval endpoint calls `ApprovalAuthorizer` before persisting a decision.
Missing role or approved scope fails closed with HTTP 403. Cross-tenant access is
hidden as HTTP 404; a stale or mismatched approval ID is rejected as invalid
input without executing the write.

When `ABAR_OIDC_SCOPE_FIELD` is configured, the approval authorizer can enforce
regional role suffixes such as `transfer-approver:华东` or
`transfer-approver:华东,华南`. A scoped reviewer sees a pending write only when
the role covers every value in the Tool's scope array, and cannot approve a
larger scope than the role grants. The exact base role remains a global reviewer.

## Diagnostics

`abar runtime status` reads authenticated `/v1/runtime/status`:

- active worker count;
- Session and Message counts by state for the caller;
- provider configuration validation;
- state-store write probe.

The Server also exposes operational endpoints on its loopback listener:

- `/healthz`: process liveness;
- `/readyz`: Provider configuration and storage readiness;
- `/metrics`: Prometheus counters plus global Session/Message state gauges.

HTTP errors, worker completion, worker failure, and approval decisions are
written as structured JSON log events. Tokens, prompts, Tool arguments, and
output are not logged.

Model Prompt, Completion, and Total Token usage is accumulated across every
model call in a Message. Usage captured before approval or external waiting is
stored in the Message Checkpoint, so restart and resume do not lose accounting.
`/metrics` exposes `abar_model_tokens_total{type="prompt|completion|total"}`.

## Backup and restore

`abar state backup` acquires the Server lock, creates a `tar.gz`, and writes a
SHA-256 sidecar. It refuses to run while the Server is active.

`abar state restore` verifies archive paths and file types before extraction.
With `--replace`, the previous state directory is retained and reported in the
command output.

Refer to [DingTalk Alerts](../../../examples/merchandise-operations/DINGTALK-ALERTS.md)
for the manager-only TopN reminder
flow. Alert reports are separate from model-callable MCP Tools, and the Action
Card links to the OIDC Workbench.

## Verification

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/check.sh
bash scripts/test-merchandise-operations.sh
```

The shell suite contains one business scenario: the unified merchandise
operations Agent. Core contract, identity, runtime, and storage behavior remains
covered by Rust tests.
