# ERP/WMS Read-only Integration

English | [简体中文](ERP-WMS-INTEGRATION.zh-CN.md)

Enterprise ERP/WMS integration must be read-only by default.

## Required connection information

- MCP Streamable HTTP endpoint;
- authentication method and secret reference;
- TLS and network allowlist;
- available Tool names and input/output Schemas;
- declared effect for every Tool;
- data freshness and business time field;
- rate limits and timeout requirements.

## Initial Tool set

```text
inventory.query_store_inventory
inventory.query_sales_velocity
inventory.query_in_transit
```

All three Tools are `read`.

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

No write Tool is enabled in the read-only integration.

## Data policy

The following fields must not enter model context:

```text
cost
grossMargin
supplierPrice
```

The model and WebUI may receive projected inventory, sales, transit, and
recommendation fields. Raw sensitive values must remain outside model context.

## Acceptance

- Inventory and sales values match the ERP source for sampled stores and SKUs.
- The business-time field is present and traceable.
- Tool Schema digests remain stable during a Session.
- Cost, margin, and supplier-price fields are absent from model requests.
- ERP failures fail closed and do not turn into fabricated inventory.
- A single recommendation can be explained from source store, target store,
  SKU, quantity, coverage days, and reason codes.

Write Tools require a separate reviewed Scope and approval configuration.
