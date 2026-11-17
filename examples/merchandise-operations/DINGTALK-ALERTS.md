# DingTalk Alerts

English | [简体中文](DINGTALK-ALERTS.zh-CN.md)

DingTalk is an external notification channel. It does not execute Agent logic.
See [DingTalk Workbench Entry](../../adapters/abar-channel-dingtalk/README.md) for
the browser entry and OIDC deep-link behavior.

## Flow

```text
BI / Data Lab report
        |
        v
business-owned notification job
        |
        v
DingTalk signed action card
        |
        v
Agent Workbench (OIDC WebUI)
```

The manager receives TopN sales-decline, gross-margin-decline, and
inventory-shortage items. The card links to the configured business Agent in
the merchandise workspace, where the manager can ask for attribution, continue
a multi-turn Session, and confirm a controlled action.

## Sensitive data boundary

- Gross margin is calculated by the trusted BI or manager-only report layer.
- The DingTalk card may contain manager-authorized aggregates.
- The report endpoint is not exposed as a model-callable MCP Tool.
- Model context contains only approved inventory, sales, and alert reason data.

## Configuration

```bash
export ABAR_ALERT_REPORT_URL="http://127.0.0.1:18080/reports/merchandise-anomalies?limit=5"
export DINGTALK_WEBHOOK_URL="https://oapi.dingtalk.com/robot/send?access_token=..."
export DINGTALK_WEBHOOK_SECRET="<robot signing secret>"
export ABAR_WORKBENCH_URL="https://abar.example.com"
export ABAR_ALERT_AGENT="merchandise-operations-assistant"
```

`ABAR_ALERT_AGENT` is appended as an `agent` query parameter. Use the Agent
that owns the reminder workflow. The merchandise demo uses
`merchandise-operations-assistant`, which infers allocation, replenishment, or
transfer from the manager's question.

`DINGTALK_ALLOW_HTTP=true` is available only for local integration tests.

ABAR provides the signed DingTalk adapter. The report schedule, manager
audience, and notification job remain business-owned.

## Boundary

This adapter currently covers outbound manager reminders and Workbench entry.
Inbound DingTalk chatbot messages and card callbacks require an enterprise
DingTalk application, app credentials, signature verification, and a
server-to-server identity mapping to create per-user ABAR Sessions.
