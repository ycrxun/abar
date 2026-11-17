# Security

English | [简体中文](SECURITY.zh-CN.md)

Do not report vulnerabilities through public issues. Use the repository's
private security advisory channel once the remote repository is configured.

Security-sensitive areas include:

- Artifact and Source Bundle path traversal.
- Skill resource escape and symlink handling.
- Tool allowlist expansion.
- MCP authentication and Schema drift.
- Local Server token and loopback exposure.
- Secret leakage through logs, Events, and model requests.
- Write-tool retries when the external effect is unknown.
