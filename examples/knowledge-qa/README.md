# Knowledge Q&A Agent

English | [简体中文](README.zh-CN.md)

This example defines a read-only Q&A Agent grounded in a small product-service
knowledge base. It has no Tool bindings and does not create business actions.

## What it demonstrates

- one clear business responsibility: answer questions from approved knowledge;
- a Versioned Agent definition with Prompt, Skill, rules, and Knowledge Source;
- evidence-based answers without exposing internal document names;
- insufficient-evidence behavior instead of invented business facts;
- a separate Codex-style Agent WebUI bound to this Agent;
- a compact WebUI that hides the context evidence rail and approval navigation;
- no MCP Tool, write operation, or approval path.

## Project layout

```text
knowledge-qa/
├── agent.yaml
├── scope.yaml
├── behavior.yaml
├── knowledge.yaml
├── tools.yaml
├── prompt/system.md
├── skills/grounded-qa/SKILL.md
├── rules/answer-boundaries.md
├── knowledge/product-handbook/
│   ├── store-service.md
│   ├── warranty-policy.md
│   └── return-policy.md
├── config.test.yaml
├── message.providers.test.yaml
├── config.ollama.yaml
├── message.providers.ollama.yaml
├── webui-view.yaml
└── test.sh
```

Knowledge documents are example content for a fictional retail service policy.
Replace them with the business's approved source material.

## Responsibility

The Agent answers questions about:

- standard store operating hours and pickup service;
- product warranty periods and required evidence;
- return and exchange policy.

It does not query live inventory, orders, prices, refunds, or personal data.
Questions outside the knowledge base receive an explicit evidence limitation.
The View sets `context.enabled: false`, so users see the conversation without
the plan, Tool evidence, Composition, or Event rail. Those records remain
available in the persisted Message and audit data.
It also sets `approval.enabled: false` because this Agent has no write actions.

## Run with Ollama

Start Ollama and pull the configured model:

```bash
ollama pull llama3.2:latest
```

Configure ABAR:

```bash
export ABAR_STATE_DIR=/tmp/abar-knowledge-qa
export ABAR_CONFIG="$PWD/examples/knowledge-qa/config.ollama.yaml"
export ABAR_MESSAGE_CONFIG="$PWD/examples/knowledge-qa/message.providers.ollama.yaml"

abar publish examples/knowledge-qa
abar server start --daemon

abar-webui \
  --agent knowledge-qa-assistant \
  --view examples/knowledge-qa/webui-view.yaml
```

Open <http://127.0.0.1:19090> and try:

```text
标准门店营业时间是什么？
商品保修需要提供什么材料？
无理由退货和质量问题退换货分别有什么规则？
```

## Automated verification

```bash
bash examples/knowledge-qa/test.sh
```

The test starts a deterministic OpenAI-compatible model endpoint, publishes the
Agent, executes a Message, and verifies:

- the answer completes without a Tool;
- knowledge evidence contains `store-service.md`;
- the output is grounded in the sample policy;
- Token usage is recorded;
- Agent WebUI loads the bound Q&A View.

## Change knowledge

1. Add or update Markdown or text documents under `knowledge/product-handbook/`.
2. Keep source documents approved for the audience using the Agent.
3. Update the Prompt or rules when the answer boundary changes.
4. Run `test.sh`.
5. Publish a new Revision.

Knowledge content changes are captured in each Message through the actual
document IDs, versions, digests, and retrieval timestamps.

## Answer boundary

- Retrieved document evidence remains in Message Composition for audit but is
  not repeated in the user answer.
- Missing evidence must be stated explicitly.
- The Agent must not invent policy or live business data.
- Sensitive cost, margin, and supplier-price classes are denied in the Agent
  Scope.
- Questions needing live operations require a different Agent with reviewed
  read Tools or a reviewed write workflow.
