# Skills

English | [简体中文](../../zh-CN/guides/SKILLS.md)

Skills are versioned behavior resources referenced by an Agent Revision.

For the full business definition workflow, see
[Define a Business Agent](AUTHORING.md). This page is the Skill format reference.

```text
skills/<skill-name>/
└── SKILL.md
```

The current version supports exactly `SKILL.md` per Skill. Symlinks and parent
traversal are rejected.

`behavior.yaml` binds each Skill ID, version, and project-relative directory:

```yaml
skills:
  - id: merchandise-intent-routing
    version: 1.0.0
    path: skills/merchandise-intent-routing
```

Publishing computes the directory digest and materializes the content beside
the immutable Revision. A Skill content change requires a new Revision.

The Message records the exact Skill versions and digests used. Skills with
`disable-model-invocation: true` are loaded only when requested by a Message.
