# ERP/WMS 只读接入

[English](ERP-WMS-INTEGRATION.md) | 简体中文

企业 ERP/WMS 接入默认必须保持只读。

## 必需连接信息

- MCP Streamable HTTP Endpoint；
- 认证方式和 Secret 引用；
- TLS 和网络白名单；
- Tool 名称及输入/输出 Schema；
- 每个 Tool 声明的 effect；
- 数据新鲜度和业务时间字段；
- 限流和超时要求。

## 首批 Tool

```text
inventory.query_store_inventory
inventory.query_sales_velocity
inventory.query_in_transit
```

三个 Tool 均为 `read`。

```yaml
tools:
  mcpServers:
    - name: inventory
      endpoint: https://erp.example.com/mcp
      authRef: env://ABAR_ERP_MCP_TOKEN
      tools:
        query_store_inventory: read
        query_sales_velocity: read
        query_in_transit: read
```

只读接入不启用任何写 Tool。

## 数据策略

以下字段不得进入模型上下文：

```text
cost
grossMargin
supplierPrice
```

模型和 WebUI 只能接收投影后的库存、销量、在途和建议字段。原始敏感值必须
保留在模型上下文之外。

## 验收

- 抽样门店和 SKU 的库存、销量与 ERP 一致。
- 业务时间字段存在且可追溯。
- 同一 Session 内 Tool Schema Digest 保持稳定。
- 成本、毛利和供应商价格不出现在模型请求中。
- ERP 失败时失败关闭，不能生成虚构库存。
- 每条建议都能通过调出门店、调入门店、SKU、数量、覆盖天数和原因码解释。

写 Tool 需要独立的 Scope Review 和审批配置。
