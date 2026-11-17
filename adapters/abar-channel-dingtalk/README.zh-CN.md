# 钉钉工作台入口

[English](README.md) | 简体中文

本文定义从钉钉工作台进入 OIDC Agent WebUI 的接入方式，不依赖钉钉入站消息、
机器人回调或互动卡片回调。

## 需要配置的内容

企业钉钉管理员配置企业内部应用，将 WebUI 公网地址或 Agent 专用路由设置为
应用首页：

```text
https://abar.example.com/
https://abar.example.com/?agent=<agent-name>
```

WebUI 必须能够从钉钉桌面端和移动端 WebView 通过 HTTPS 访问。IdP Client
必须登记精确的回调地址：

```text
https://abar.example.com/auth/callback
```

不带 `agent` 参数时进入配置的默认 Agent。多个独立 Agent 使用各自的路由部署。

## 登录行为

未登录时，工作台入口会跳转到配置的 OIDC Provider。WebUI 把原始同源 Agent
地址绑定到 OIDC 登录 State，只有登录成功后才返回原地址：

```text
钉钉工作台
        |
        v
/?agent=<agent-name>
        |
        v
OIDC Authorization Code + PKCE
        |
        v
/?agent=<agent-name>
```

外部 URL、`//` 开头的相对 URL、控制字符、反斜杠和超长返回路径都会被拒绝。
不同浏览器标签页也会保留各自要打开的 Agent。

## 第一步企业侧输入

接入需要以下内容：

- WebUI 公网 HTTPS Origin；
- OIDC issuer 和 WebUI client ID；
- 精确的 OIDC Redirect URI；
- 企业内部应用注册所需的钉钉 `CorpId`、`AgentId` 和 `AppKey`；
- 测试用户及其 OIDC 身份。

`AppSecret`、回调 Token 和回调加密 Key 在接入入站机器人或卡片回调前都不需要。

## 本地 Keycloak

本地 OIDC Provider 为 Keycloak：

```bash
bash scripts/keycloak-local.sh start
```

默认连接信息：

```text
Issuer: http://127.0.0.1:18093/realms/abar
Realm: abar
WebUI Client: abar-test
WebUI Redirect: http://127.0.0.1:19093/auth/callback
```

测试用户：

```text
alice / alice-password      transfer-approver
bob / bob-password          viewer
charlie / charlie-password  viewer, 不同 tenant
```

Keycloak 使用 `quay.io/keycloak/keycloak:26.6.1`，并把
`tests/keycloak/keycloak-dingtalk-provider.jar` 只读挂载到
`/opt/keycloak/providers/keycloak-dingtalk-provider.jar`。需要验证其他 JAR
时可通过 `ABAR_KEYCLOAK_DINGTALK_PROVIDER_JAR` 覆盖路径。启动脚本会检查
Keycloak 日志，确认 `DingTalkIdentityProviderFactory` 已加载。

启动包含 Keycloak、ABAR Server 和 Agent WebUI 的本地工作台：

```bash
bash scripts/workbench-local.sh start
```

入口：

```text
http://127.0.0.1:19093/
http://127.0.0.1:19093/?agent=<agent-name>
```

该入口模拟钉钉工作台打开的目标 URL；本地环境本身不能被远程钉钉客户端访问。

## 验收

1. 在没有 WebUI 会话的情况下打开工作台入口。
2. 完成 OIDC 登录。
3. 确认最终 URL 仍然包含 `?agent=<name>`。
4. 确认打开的是目标 Agent，并加载对应 View Pack。
5. 分别在钉钉桌面端和移动端重复验证。

当前 WebUI 的安全 Header 禁止 iframe 嵌入，因此本方案默认钉钉工作台使用
顶层 WebView 打开地址。如果目标钉钉客户端实际使用 iframe，需要单独评审并
调整嵌入策略，不能直接放宽限制。

## 验证

使用上方的验收清单验证实际部署的 Agent WebUI。
