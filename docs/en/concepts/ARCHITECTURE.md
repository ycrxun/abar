# Architecture

English | [简体中文](../../zh-CN/concepts/ARCHITECTURE.md)

```text
abar CLI / WebUI
        |
        v
    Agent Server
        |
        +-- Agent Revision Store
        +-- Session / Message Store
        +-- Runtime Events
        +-- Message Worker
                |
                +-- ModelProvider
                +-- ToolRegistry / MCP
                +-- KnowledgeProvider
                +-- DataPolicy
```

## Responsibilities

| Component | Responsibility |
|---|---|
| `abar-api` | Stable contracts, schemas, validators |
| `abar-core` | Revision Build, Session/Message state, workers, providers |
| `abar-cli` | Publish, Revision review, Server, Session, Message, diagnostics |
| `abar-webui` | OIDC BFF, Agent conversation, business view, and approval interface |
| `abar-platform-console` | Agent catalog, Revision governance, Scope Review, and Runtime aggregates |
| `adapters` | Replaceable Model and Tool providers |
| `examples` | Executable unified merchandise-operations integration |

## Build

`abar publish` reads a Revision project, computes behavior and scope digests,
validates the Revision, and stores it immutably.

- Behavior-only changes become the current Revision.
- Scope expansion becomes `waiting_review`.
- Scope Review evidence is bound to the previous and candidate Revisions.

## Runtime

1. Client creates a long-lived Session.
2. Client creates a Message.
3. Server resolves the current Revision and Provider Bundle.
4. Server persists `queued` Message evidence.
5. Worker executes model and Tool iterations.
6. Writes suspend for exact approval.
7. Unknown writes suspend for exact idempotency reconciliation.
8. Worker records Tool results, checkpoint, output, and Runtime Events.

## Recovery

State required for recovery lives outside the worker:

- Session and Message JSON records;
- Execution Composition;
- approval and pending external-call evidence;
- checkpoint and Tool results;
- append-only Runtime Events.

Startup recovers `queued`, `running`, and `waiting_external` Messages.
Approval waits require a new decision.

## Boundaries

- Workers are in-process. Distributed cross-process leases are outside the thin
  core.
- Diagnostics are read-only and caller-scoped.
- WebUI keeps browser sessions and IdP tokens behind an OIDC BFF boundary.
- Organization IAM remains an external `AuthProvider`; approval visibility and
  decision authority are enforced by `ApprovalAuthorizer`.
- MCP Streamable HTTP is the bundled Tool transport.
