---
name: merchandise-intent-routing
description: 从自然语言识别配货、补货或调拨意图，并选择对应工具。
allowed-tools:
  - inventory.calculate_allocation_plan
  - inventory.calculate_replenishment_plan
  - inventory.calculate_transfer_recommendations
  - inventory.query_store_inventory
  - inventory.query_sales_velocity
  - inventory.query_in_transit
disable-model-invocation: false
version: 1.0.0
---

# 商品运营意图路由

根据业务对象和库存流向判断，不只依赖单个关键词：

- 中心仓到门店，强调新品、首铺、新店或主动分配：配货。
- 中心仓到门店，强调覆盖天数、销量、在途、陈列量或持续消耗：补货。
- 门店到门店，强调过量、缺货、同城或区域平衡：调拨。

如果一句话同时包含多个业务目标，先列出识别结果并询问本次先处理哪一项。
如果信息不足，优先调用只读查询帮助澄清，不得创建任何业务单。
