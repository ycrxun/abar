# Skill

[English](../../en/guides/SKILLS.md) | 简体中文

Skill 是 Agent Revision 引用的版本化 Behavior 资源。

完整业务定义流程见[业务定义 Agent](AUTHORING.md)。本页只说明 Skill 文件格式。

```text
skills/<skill-name>/
└── SKILL.md
```

当前版本每个 Skill 只支持 `SKILL.md`，并拒绝符号链接和父目录穿越。

`behavior.yaml` 绑定 Skill ID、版本和项目相对目录：

```yaml
skills:
  - id: merchandise-intent-routing
    version: 1.0.0
    path: skills/merchandise-intent-routing
```

发布时计算目录摘要，并把内容物化到不可变 Revision 旁。Skill 内容变化需要新
Revision。

Message 记录实际使用的 Skill 版本和摘要。`disable-model-invocation: true` 的
Skill 只会在 Message 显式请求时加载。
