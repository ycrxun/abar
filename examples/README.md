# Examples

English | [简体中文](README.zh-CN.md)

This directory is the single home for both minimal examples and end-to-end
business examples.

Rules:

- Minimal examples should avoid vertical business logic.
- Business workflows live in a named subdirectory and must not enter `abar-core`.
- Every example must be runnable from the repository without hidden setup.
- Business examples should document data assumptions, approvals, and safety
  boundaries.

To define a new business Agent, start with the
[Agent authoring guide](../docs/en/guides/AUTHORING.md).

## Available examples

| Example | Scope |
|---|---|
| [merchandise-operations](merchandise-operations/README.md) | Conversation-first allocation, replenishment and transfer, ERP/WMS integration, and DingTalk TopN alerts |
| [knowledge-qa](knowledge-qa/README.md) | Read-only Q&A grounded in versioned business knowledge, with a dedicated Agent WebUI |

The merchandise example also contains its deterministic OpenAI-compatible model
and mock MCP server, so business vocabulary and behavior stay outside the
generic runtime and adapters.
