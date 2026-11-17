# 知识问答 Agent

[English](README.md) | 简体中文

这个案例定义一个只读的知识问答 Agent，知识范围来自一组产品服务资料。它没有
Tool 绑定，也不会创建业务动作。

## 展示内容

- 一个清晰的业务职责：依据获批知识回答问题；
- 版本化 Agent 定义，包括 Prompt、Skill、规则和 Knowledge Source；
- 有知识依据，但不向用户暴露内部文档名称；
- 证据不足时明确说明，而不是编造业务事实；
- 绑定该 Agent 的独立 Codex 式 Agent WebUI；
- 隐藏上下文证据栏和审批导航的简洁 WebUI；
- 不依赖 MCP Tool、写操作或审批链路。

## 项目结构

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

知识文件是为示例构造的虚构零售服务政策。实际使用时，应替换为业务方批准的资料。

## 职责

该 Agent 可以回答：

- 标准门店营业时间和到店自提服务；
- 商品保修期限及所需材料；
- 退换货政策。

它不查询实时库存、订单、价格、退款进度或个人数据。知识库无法支持的问题，Agent
必须明确说明证据不足。

View 设置了 `context.enabled: false`，因此用户只看到对话，不展示计划、Tool
证据、Composition 或事件栏。这些记录仍保存在持久化 Message 和审计数据中。
该 View 同时设置 `approval.enabled: false`，因为此 Agent 没有写操作。

## 使用 Ollama 运行

启动 Ollama 并准备配置中的模型：

```bash
ollama pull llama3.2:latest
```

配置 ABAR：

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

打开 <http://127.0.0.1:19090>，可以尝试：

```text
标准门店营业时间是什么？
商品保修需要提供什么材料？
无理由退货和质量问题退换货分别有什么规则？
```

## 自动验证

```bash
bash examples/knowledge-qa/test.sh
```

测试会启动确定性的 OpenAI-compatible 模型端点，发布 Agent、执行 Message，并验证：

- 不调用任何 Tool 即可完成回答；
- 知识证据包含 `store-service.md`；
- 输出受到示例政策约束；
- Token 用量被记录；
- Agent WebUI 正确加载绑定的问答 View。

## 修改知识

1. 在 `knowledge/product-handbook/` 下新增或修改 Markdown、文本文件。
2. 只保留允许目标用户访问的已批准资料。
3. 回答边界变化时，同时更新 Prompt 或规则。
4. 运行 `test.sh`。
5. 发布新的 Revision。

知识内容变化会通过实际使用的文档 ID、版本、摘要和检索时间记录到每条 Message。

## 回答边界

- 检索证据保留在 Message Composition 中供审计，但不在用户回答中重复。
- 缺少证据时必须明确说明。
- 不得编造政策或实时业务数据。
- Agent Scope 禁止成本、毛利和供应商价格类别。
- 需要实时业务查询的问题，应使用具有已评审只读 Tool 或写审批流程的其他 Agent。
