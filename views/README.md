# WebUI View Packs

English | [简体中文](README.zh-CN.md)

A View Pack is declarative WebUI presentation configuration. It does not
enter Agent Runtime.

The built-in default View preserves the Codex-style shell when no independent
View Pack is configured.

A View Pack controls:

- optional task suggestion fields and prompt templates;
- the post-plan confirmation label and Message template;
- business metrics, tables, and column formats;
- display titles and labels.
- whether the context rail is displayed.
- whether the approval inbox and approval UI are displayed.

View Packs do not execute scripts and cannot expand tool permissions or bypass
approval.

Every View Pack reuses the same Codex-style shell, Session sidebar, central
conversation, single Composer, and context rail. A View Pack only adds business
shortcuts, metrics, tables, and copy to that shared interaction model; it cannot
replace the shell.

The context rail is shown by default. A View can hide presentation-only
context evidence with:

```yaml
context:
  enabled: false
```

Execution evidence remains persisted and auditable; only its presentation is
disabled.

Approval UI is shown by default. A View for an Agent without write actions can
hide it with:

```yaml
approval:
  enabled: false
```

This only removes presentation; the Server still enforces approval for any
write Tool that exists in the Agent Revision.

The WebUI is conversation-first. Free-form input is always available; View
Pack actions are optional shortcuts and do not restrict the sequence of
questions or adjustments in a Session.

Each WebUI is a single-Agent deployment. Multiple Agents run as separate WebUI
instances. A View Pack must declare one Agent binding; the default View is used
when no View Pack is supplied. See
[Agent View Configuration](../docs/en/guides/AGENT-VIEW.md).

```yaml
binding:
  agent: merchandise-operations-assistant
```

Current example:

- `merchandise-operations/webui-view.yaml`
