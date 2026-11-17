---
name: grounded-qa
description: 依据检索到的知识给出简洁、用户可读的业务回答。
disable-model-invocation: false
version: 1.0.0
---

# Grounded Q&A

1. 提取问题中的产品、政策或流程关键词。
2. 使用最多四个相关文档片段。
3. 先直接回答问题。
4. 不向用户展示文档名称、路径或摘要。
5. 如果证据不足，说明缺少什么信息，并建议用户补充范围。
