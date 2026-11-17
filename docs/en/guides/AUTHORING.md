# Define a Business Agent

English | [简体中文](../../zh-CN/guides/AUTHORING.md)

This guide is for business owners and business Agent designers. By the end, you
should be able to turn a job responsibility into a publishable, testable, and
governable Agent.

The platform provides identity, execution, approval, recovery, metering, and
scheduling. The business defines why the Agent exists, what it knows, what it
may do, and when it must stop.

## 1. Decide whether a new Agent is needed

An Agent should represent a stable business responsibility. Split when there is
a distinct user group or job, a different Tool or write scope, a different data
boundary, a different approver group, or an independent owner and release
cadence.

Do not split an Agent merely because the UI has different buttons. Allocation,
replenishment, and transfer can remain one merchandise Agent when they share
users, business objects, data, and approval boundaries.

## 2. Business definition vs platform responsibility

| Business defines | Platform provides |
|---|---|
| Agent goal and boundary | identity, Tenant, and authorization enforcement |
| Prompt, Skills, rules, and Knowledge | Build, immutable Revisions, and Scope Review |
| Tool allowlist and read/write scope | Tool governance, approval, idempotency, recovery |
| allowed and denied data classes | DataPolicy projection and model-context isolation |
| model capability and scale | concrete Model Provider and execution implementation |
| business shortcuts and result display | Agent WebUI, Sessions, and live updates |
| business scope required by a write approval | reviewer identity, role, visibility, and enforcement |

Business definition does not include containers, processes, checkpoint formats,
retry counts, or vendor SDKs.

## 3. Agent project layout

```text
my-agent/
├── agent.yaml
├── scope.yaml
├── behavior.yaml
├── knowledge.yaml
├── tools.yaml
├── prompt/
│   └── system.md
├── skills/
│   └── <skill-id>/
│       └── SKILL.md
└── rules/
    └── <rule-id>.md
```

Start from the merchandise example:

```bash
cp -R examples/merchandise-operations ../agents/my-business-agent
```

## 4. Define identity and execution limits

`agent.yaml`:

```yaml
apiVersion: abar/v1
kind: Agent
metadata:
  name: merchandise-operations-assistant
spec:
  modelClass: general-tool-use
  maxSteps: 40
  scope: scope.yaml
  behavior: behavior.yaml
  knowledge: knowledge.yaml
  tools: tools.yaml
```

`metadata.name` is the stable identity used by Agent WebUI, Sessions, and
deployment routes.

`modelClass` declares a capability class rather than a vendor model. `maxSteps`
bounds model and Tool iterations for one Message.

## 5. Define the permission boundary

`scope.yaml` answers "what may this Agent ever touch?":

```yaml
tools:
  read:
    - inventory.query_*
  write:
    - inventory.create-transfer-order
skills:
  - transfer-decision
knowledgeSources:
  - transfer-policy
data:
  allow:
    - inventory
    - sales
  deny:
    - cost
    - gross-margin
```

Rules:

- start with the minimum scope required for the responsibility;
- every `write` Tool enters the approval path;
- deny sensitive data classes explicitly;
- expanding Tool, Skill, Knowledge, or data scope requires Scope Review;
- changing Prompt, rules, or Knowledge content does not automatically expand the
  permission boundary.

Reviewers and roles are not stored in `agent.yaml`; platform IAM and
`ApprovalAuthorizer` configuration enforce them.

## 6. Define how the Agent works

`behavior.yaml` binds the Prompt, Skills, and rules:

```yaml
prompt: prompt/system.md
skills:
  - id: merchandise-intent-routing
    version: "1.0.0"
    path: skills/merchandise-intent-routing
rules:
  - id: merchandise-routing-rules
    version: "1"
    path: rules/routing-rules.md
```

The Prompt should define responsibility, business boundaries, and stop
conditions. Keep Tool schema details and platform recovery mechanics out of the
Prompt.

Use Skills for reusable working methods. Use rules for business policy and
decision boundaries. Use Knowledge for evolving policies and explanatory
documents.

## 7. Bind Tools and Knowledge

`tools.yaml` declares the Tools the Agent actually uses:

```yaml
tools:
  - id: inventory.query-inventory
    versionRange: ">=4.7 <5.0"
    effect: read
  - id: inventory.create-transfer-order
    versionRange: ">=6.2 <7.0"
    effect: write
```

`knowledge.yaml` declares Knowledge Sources:

```yaml
sources:
  - id: transfer-policy
    required: true
    retrieval:
      topK: 4
```

Platform or integration teams provide Tool IDs, versions, and MCP services.
Business owners select allowed Tools and confirm their read/write effect. Actual
Knowledge file locations and retrieval implementation belong to runtime
configuration.

## 8. Protect sensitive data

The business declares allowed and denied classes in `scope.data`. DataPolicy
projects Tool results before they enter model context.

Example:

```yaml
data:
  allow: [inventory, sales]
  deny: [cost, gross-margin, supplier-price]
```

Integration configuration must also map real fields:

```yaml
dataPolicy:
  fields:
    - path: unitCost
      dataClass: cost
    - path: grossMargin
      dataClass: gross-margin
```

Declaring a denied class without field mappings does not create a verifiable
data boundary.

## 9. Define the WebUI business view

View is optional. Without one, Agent WebUI uses the built-in Codex-style shell.

A View may add:

- shortcut fields and prompt templates;
- structured confirmation copy;
- business metrics, tables, and formats;
- approval and result labels.
- whether the context evidence rail is displayed.
- whether approval navigation is displayed for Agents without write actions.

A View cannot change Tool Scope, DataPolicy, approval authority, or Message
state.

## 10. Verify the Agent

Start the example's deterministic model and Mock MCP:

```bash
export ABAR_STATE_DIR=/tmp/my-agent
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"

cargo run -p abar-merchandise-demo-model
cargo run -p abar-mock-mcp-server

abar publish ../agents/my-business-agent --json
```

Verify at least:

- a typical request selects the correct Tool;
- incomplete requests are clarified;
- unauthorized Tools and sensitive fields fail closed;
- write Tools do not execute before approval;
- repeated writes use the same idempotency key;
- the model does not invent missing business data.

Business acceptance should include normal, ambiguous, rejected, sensitive-data,
and external-failure cases.

## 11. Publish and review Scope

Behavior-only changes can become the Current Revision. Resource expansion leaves
the Revision as a candidate for Scope Review.

CLI:

```bash
abar revision approve <agent>:<revision> \
  --reviewer <reviewer> \
  --comment "approved by business and security"
```

Platform Console can also inspect the Scope and approve or reject the candidate.

## 12. Deploy Agent WebUI

```bash
abar-webui \
  --agent <agent-name> \
  --view views/<business>/webui-view.yaml \
  --addr 127.0.0.1:19093
```

Each Agent has its own WebUI. Business users enter the correct Agent from the
enterprise portal, DingTalk, or another channel; they do not choose an Agent
inside the page.

## 13. Pre-release checklist

- one clear responsibility and owner;
- Prompt, Skills, rules, and Knowledge do not conflict;
- Tool allowlist is minimally sufficient;
- every write Tool has exact approval and idempotency;
- denied data fields are mapped in runtime configuration;
- normal, ambiguous, unauthorized, and external-failure cases are verified;
- View contains presentation only;
- expanded Scope is reviewed by the correct role;
- WebUI address and login roles are confirmed.
