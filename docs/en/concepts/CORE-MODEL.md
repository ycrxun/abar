# ABAR Core Model

English | [简体中文](../../zh-CN/concepts/CORE-MODEL.md)

ABAR is a thin Agent Build/Runtime core.

```text
Agent
├── Agent Revision
├── Session
│   └── Message
└── Tool Service
```

## 1. Agent Revision

Build resolves, validates, and seals one immutable Agent Revision:

```text
Agent Revision
├── resourceScope
├── behavior
├── knowledgeSources
└── toolBindings
```

`resourceScope` fixes Tool IDs and maximum effects, Skill IDs, Knowledge Source
IDs, data classes, model class, and maximum steps.

`behavior` fixes Prompt, Skill, and business-rule references with digests.
Ordinary knowledge documents may evolve; each Message records the actual
documents, versions, digests, and retrieval times used.

Resource expansion creates a candidate Revision and requires Scope Review.
Behavior-only changes may become current automatically.

## 2. Session

A Session is a long-lived context container owned by one authenticated tenant
and user. It is not a process, container, or single task.

```text
active <-> idle
idle -> closed -> archived
```

Only one Message may actively occupy a Session at a time.

## 3. Message

A Message is one execution of one Agent Revision.

```text
queued -> running
running -> waiting_approval | waiting_external | completed | failed | cancelled
waiting_approval -> running | failed | cancelled
waiting_external -> running | failed | cancelled
```

Every Message records input, requested Skills, Agent Revision, actual model,
Tool, Skill, Knowledge and Schema versions, Tool results, approval evidence,
checkpoint, output, and Runtime Events.

Historical Messages never change when the current Agent Revision advances.

## 4. Execution

```text
Message
  -> Context Assembly
  -> Model
  -> Tool Intent
  -> Scope and DataPolicy
  -> Tool Execution
  -> Evidence / Approval / External Wait
  -> Result or Recovery
```

Message creation persists `queued` before dispatching a background worker.
Cancellation is cooperative and commits at the next model or Tool boundary.
Confirmed Tool evidence survives cancellation.

`waiting_external` persists the exact Tool ID, arguments, and idempotency key.
A missing write remains waiting and is never silently reissued. A read may be
retried safely from the same persisted intent.

Startup recovery resumes `queued`, `running`, and `waiting_external` Messages.
`waiting_approval` remains suspended until a decision arrives.

## 5. Security invariants

- models produce intentions, not execution authority;
- Tools are checked against Revision scope and effect;
- writes require exact-call approval and idempotency;
- sensitive data is classified and projected before model context;
- denied classifications fail closed;
- logs and diagnostics do not expose Tokens, secrets, or host paths.

## 6. Extensions

The stable extension boundaries are `ModelProvider`, `ToolRegistry`,
`KnowledgeProvider`, `DataPolicy`, `ContentProvider`, and `AuthProvider`.
Adapters depend on `abar-api` and its conformance validators.

Approval authority is a separate `ApprovalAuthorizer` boundary. Identity
authentication never implies permission to approve a business write.
The approval inbox exposes only minimal pending-call evidence within the same
Tenant and reviewer role; it does not grant access to the requester's Session
history or output.
