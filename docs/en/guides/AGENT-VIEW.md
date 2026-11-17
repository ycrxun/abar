# Agent View Configuration

English | [简体中文](../../zh-CN/guides/AGENT-VIEW.md)

An Agent View is declarative presentation configuration for one Agent WebUI. It
does not enter Agent Runtime.

A WebUI binds one Agent, and a View binds that same Agent. When no View is
provided, the WebUI uses the built-in default View. A View changes how
information is presented; it does not change identity, permissions, Tool Scope,
DataPolicy, approvals, or Message transitions.

## 1. Minimal configuration

Create `agent-view.yaml`:

```yaml
apiVersion: webui/v1
kind: WebUIView
binding:
  agent: example-assistant
metadata:
  name: example-view
  title: Example Assistant
task:
  title: Start a conversation
  chatPlaceholder: Describe a goal
  fields: []
  actions: []
```

Start the WebUI with the same Agent:

```bash
abar-webui \
  --agent example-assistant \
  --view ./agent-view.yaml
```

The WebUI refuses to start when `binding.agent` and `--agent` differ.

## 2. Complete example

The following configuration demonstrates the presentation fields currently
supported by `webui/v1`:

```yaml
apiVersion: webui/v1
kind: WebUIView
binding:
  agent: example-assistant
metadata:
  name: example-view
  title: Example Assistant
  subtitle: Conversation, structured results, and execution evidence
  badges:
    - Read-only
    - Example

task:
  title: What would you like to handle today?
  chatPlaceholder: Describe a goal or choose a shortcut
  fields:
    - name: scope
      label: Scope
      type: select
      defaultValue: all
      options: [all, group-a, group-b]
    - name: note
      label: Additional requirements
      type: textarea
      placeholder: Optional
  actions:
    - id: summarize
      label: Generate summary
      primary: true
      inputTemplate: "Summarize tasks within {{scope}}. {{note}}"

detail:
  titleTemplate: "{{title}}"
  titleToolId: example.read_summary
  titlePath: title

confirmation:
  label: Confirm execution
  inputTemplate: Confirm the business proposal shown in this conversation.
  requestedSkills:
    - execution-guidance

metrics:
  - label: Processed
    toolId: example.read_summary
    path: summary.processed
  - label: Total
    toolId: example.read_summary
    path: summary.total

panels:
  - type: table
    title: Result details
    toolId: example.read_summary
    rowsPath: items
    meta:
      - label: Data time
        path: dataAsOf
    columns:
      - label: Name
        path: name
      - label: Status
        path: status
      - label: Quantity
        path: quantity

context:
  enabled: true

approval:
  enabled: true
  title: Waiting for approval
  approveLabel: Approve
  rejectLabel: Reject
```

## 3. Configuration fields

### `binding`

```yaml
binding:
  agent: example-assistant
```

`agent` must be the stable Agent name from the Agent Revision and must exactly
match the WebUI `--agent` argument.

### `metadata`

```yaml
metadata:
  name: example-view
  title: Example Assistant
  subtitle: Conversation, structured results, and execution evidence
  badges:
    - Read-only
```

| Field | Required | Purpose |
|---|---|---|
| `name` | yes | stable View name |
| `title` | yes | page, Agent, and floating-composer title |
| `subtitle` | no | Agent description |
| `badges` | no | lightweight labels next to the user area |

### `task`

`task` defines the empty state and shortcut input. Free-form input is always
available; shortcuts do not impose a required workflow.

```yaml
task:
  title: What would you like to handle today?
  chatPlaceholder: Describe a goal
  fields:
    - name: scope
      label: Scope
      type: select
      defaultValue: all
      options: [all, group-a, group-b]
  actions:
    - id: summarize
      label: Generate summary
      primary: true
      inputTemplate: "Summarize tasks within {{scope}}."
```

Field constraints:

- `fields[].name` starts with a letter and contains only letters, digits, `_`,
  and `-`;
- `type` is `text`, `textarea`, or `select`;
- a `select` `defaultValue` should be one of its `options`;
- `actions[].id` starts with a lowercase letter and contains only lowercase
  letters, digits, `_`, and `-`;
- `inputTemplate` references field values with `{{fieldName}}`.

### `detail`

`detail` controls the conversation-detail title and may read a value from one
Tool result:

```yaml
detail:
  titleTemplate: "{{title}}"
  titleToolId: example.read_summary
  titlePath: title
```

When `titleToolId` or `titlePath` is absent, the title falls back to a generic
task title.

### `confirmation`

`confirmation` defines the confirmation action and Message copy:

```yaml
confirmation:
  label: Confirm execution
  inputTemplate: Confirm the business proposal shown in this conversation.
  requestedSkills:
    - execution-guidance
```

- `label` and `inputTemplate` are required;
- `requestedSkills` lists Skills available to the confirmation Message;
- the business confirmation fields come from Tool results; the platform only
  verifies that they exactly match Message evidence;
- the internal `[structured confirmation]` prefix and business confirmation
  fields are never rendered to users.

### `metrics`

`metrics` extracts lightweight values from Tool results:

```yaml
metrics:
  - label: Processed
    toolId: example.read_summary
    path: summary.processed
```

`path` is a dot-separated JSON path. `toolId` selects the latest matching Tool
result in the current Message.

### `panels`

The current Panel type is `table`:

```yaml
panels:
  - type: table
    title: Result details
    toolId: example.read_summary
    rowsPath: items
    meta:
      - label: Data time
        path: dataAsOf
    columns:
      - label: Name
        path: name
      - label: Source and target
        paths: [source, target]
        format: "{{source}} -> {{target}}"
      - label: Reasons
        path: reasons
        join: ", "
```

Column options:

| Field | Purpose |
|---|---|
| `path` | read one value |
| `paths` | read several values |
| `format` | compose text with `{{path}}` placeholders |
| `join` | join array values |
| `className` | attach a CSS class |

### `context`

The context rail is shown by default. A read-only or presentation-light Agent
may hide it:

```yaml
context:
  enabled: false
```

This changes presentation only. Persisted Composition, Tool results, and Runtime
Events remain available for audit.

### `approval`

Approval UI is shown by default. An Agent without write actions may hide
approval navigation:

```yaml
approval:
  enabled: false
```

This changes presentation only. As long as a write Tool exists in the Agent
Revision, the Server still enforces approval.

Approval copy is configurable:

```yaml
approval:
  enabled: true
  title: Waiting for approval
  approveLabel: Approve
  rejectLabel: Reject
```

## 4. Default merging

A View incrementally overrides the default View:

- missing object fields inherit from the default View;
- arrays and scalar values replace their defaults;
- without `--view`, the built-in default View is used;
- the default View binds the Agent name passed to WebUI startup.

Business Views therefore do not need to redeclare the shell, Session list,
Composer, approval card, or context rail.

## 5. Security boundary

A View cannot:

- change authentication, Tenant, user identity, or Agent binding;
- expand Tool Scope or DataPolicy;
- bypass approval;
- change Message transitions;
- execute scripts or template code;
- read another Agent's Sessions or Tool results.

Unknown fields or structures that do not satisfy the Schema are invalid.

## 6. Start and verify

With a custom View:

```bash
abar-webui \
  --agent example-assistant \
  --view ./agent-view.yaml \
  --addr 127.0.0.1:19090
```

With the default View:

```bash
abar-webui \
  --agent example-assistant \
  --addr 127.0.0.1:19090
```

Inspect the loaded View:

```bash
curl -fsS http://127.0.0.1:19090/api/view | jq
```

Verify:

1. `binding.agent` matches `--agent`;
2. `metadata.title` and shell copy are correct;
3. shortcuts fill the Composer without restricting free-form input;
4. metrics, tables, and context values come from the intended Tool results;
5. hiding `context` or `approval` changes presentation only.

Machine constraints are defined by
[`webui-view.schema.json`](../../../views/schema/v1/webui-view.schema.json).
See [`views/README.md`](../../../views/README.md) for runnable examples.
