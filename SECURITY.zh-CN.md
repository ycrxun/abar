# 安全

[English](SECURITY.md) | 简体中文

不得通过公开 Issue 报告漏洞。远端仓库配置完成后，使用仓库的私有安全公告
渠道。

安全敏感范围包括：

- Artifact 和 Source Bundle 路径穿越。
- Skill 资源逃逸和符号链接处理。
- 工具白名单扩展。
- MCP 认证和 Schema 漂移。
- 本地 Server Token 和 loopback 暴露。
- 日志、Event 和模型请求中的 Secret 泄漏。
- 外部副作用未知时的写工具重试。
