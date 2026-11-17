---
name: merchandise-write-approval
description: 根据计划类型选择精确写工具并整理审批材料。
allowed-tools:
  - inventory.create_allocation_order
  - inventory.create_replenishment_order
  - inventory.create_transfer_order
disable-model-invocation: true
version: 1.0.0
---

# 商品运营写操作

只有在用户明确确认当前计划后，才能产生写工具意图。

- `planType=allocation` 使用 `inventory.create_allocation_order`
- `planType=replenishment` 使用 `inventory.create_replenishment_order`
- `planType=transfer` 使用 `inventory.create_transfer_order`

写工具参数必须使用当前计划的 `planDigest` 和 `requiredScopes`。不得自行审批、
跨计划复用工具参数或声称业务单已经创建。
