# 商品运营助手

[English](README.md) | 简体中文

这是仓库唯一业务场景：`merchandise-operations-assistant`。它是一个对话优先的
商品运营 Agent，在同一业务定义内识别配货、补货和门店间调拨意图。

将这套结构用于其他业务职责时，参见[业务定义 Agent](../../docs/zh-CN/guides/AUTHORING.md)。

生产数据接入遵循 [ERP/WMS 只读接入](ERP-WMS-INTEGRATION.zh-CN.md)说明。

经理提醒流程参见[钉钉 TopN 告警](DINGTALK-ALERTS.zh-CN.md)。

Agent 根据对话判断业务类型。配货与补货使用中心仓库存，调拨用于门店间库存
平衡。意图不明确时先澄清，不会猜测并选择计划或写工具。

## 运行

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

## 业务流程

```text
稳定快照
  -> 判断配货 / 补货 / 调拨
  -> 生成计划
  -> 多轮调整
  -> 确认执行
  -> 精确审批
  -> 幂等写入
```

同一段对话可以查询库存、生成方案、多轮调整并请求执行。三类订单保留各自独立的
Tool 和幂等证据。

## 验证

```bash
bash scripts/test-merchandise-operations.sh
```

测试使用同一个 Agent 分别生成配货、补货、调拨计划，完成确认和审批，并验证三类
订单各创建一张；同时验证意图不明确时只澄清，不调用 Tool。
