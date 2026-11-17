# 钉钉告警

[English](DINGTALK-ALERTS.md) | 简体中文

钉钉是外部通知渠道，不承载 Agent 执行逻辑。
浏览器入口和 OIDC 深链行为参见
[钉钉工作台入口](../../adapters/abar-channel-dingtalk/README.zh-CN.md)。

## 流程

```text
BI / Data Lab 报告
        |
        v
业务方通知作业
        |
        v
钉钉签名 Action Card
        |
        v
Agent 工作台（OIDC WebUI）
```

经理收到业绩下滑、毛利下滑和库存缺失 TopN。卡片跳转到商品运营工作台中
指定的业务 Agent，经理可在其中继续追问归因、进行多轮 Session，并确认受控动作。

## 敏感数据边界

- 毛利由受信任的 BI 或经理专用报告层计算。
- 钉钉卡片可以包含授权经理可见的聚合指标。
- 报告接口不作为模型可调用的 MCP Tool。
- 模型上下文只包含已批准的库存、销量和告警原因数据。

## 配置

```bash
export ABAR_ALERT_REPORT_URL="http://127.0.0.1:18080/reports/merchandise-anomalies?limit=5"
export DINGTALK_WEBHOOK_URL="https://oapi.dingtalk.com/robot/send?access_token=..."
export DINGTALK_WEBHOOK_SECRET="<机器人签名 Secret>"
export ABAR_WORKBENCH_URL="https://abar.example.com"
export ABAR_ALERT_AGENT="merchandise-operations-assistant"
```

`ABAR_ALERT_AGENT` 会作为 `agent` 查询参数写入工作台链接。应填写该提醒
归属的业务 Agent。商品运营示例使用 `merchandise-operations-assistant`，
由 Agent 根据经理的问题判断配货、补货或调拨。

`DINGTALK_ALLOW_HTTP=true` 仅用于本地集成测试。

ABAR 提供签名钉钉 Adapter；报告调度、经理范围和通知作业由业务方负责。

## 边界

当前 Adapter 覆盖经理提醒出站消息和工作台入口。钉钉聊天机器人入站消息与卡片
回调还需要企业钉钉应用、App 凭据、签名校验，以及创建每用户 ABAR Session 的
服务端身份映射。
