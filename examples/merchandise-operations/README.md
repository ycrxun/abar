# Merchandise Operations Assistant

English | [简体中文](README.zh-CN.md)

This is the single business scenario in the repository:
`merchandise-operations-assistant`, a conversation-first Agent that routes
allocation, replenishment, and store-to-store transfer requests inside one
business definition.

Use the [Agent authoring guide](../../docs/en/guides/AUTHORING.md) when adapting
this structure to another business responsibility.

Production data integration follows the
[ERP/WMS read-only integration](ERP-WMS-INTEGRATION.md) guide.

Manager notifications follow the
[DingTalk TopN alerts](DINGTALK-ALERTS.md) guide.

The Agent infers the requested operation from the conversation. Allocation and
replenishment use central-warehouse inventory; transfer balances inventory
between stores. Ambiguous requests are clarified before any planning or write
Tool is selected.

## Run

```bash
export ABAR_STATE_DIR=/tmp/abar-merchandise-operations
export ABAR_CONFIG="$PWD/examples/merchandise-operations/config.fake.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/merchandise-operations/message.providers.yaml"

abar publish examples/merchandise-operations
abar server start --daemon

abar-webui \
  --agent merchandise-operations-assistant \
  --view views/merchandise-operations/webui-view.yaml
```

## Business flow

```text
stable snapshot
  -> infer allocation / replenishment / transfer
  -> plan
  -> multi-turn adjustment
  -> confirm
  -> exact approval
  -> idempotent write
```

One conversation can explore inventory, generate a plan, adjust it over multiple
turns, and request execution. Each plan type retains separate Tool and
idempotency evidence.

## Verification

```bash
bash scripts/test-merchandise-operations.sh
```

The test uses the same Agent to create allocation, replenishment, and transfer
plans, confirms and approves them, and verifies one order of each type. It also
verifies that ambiguous requests are clarified without calling a Tool.
