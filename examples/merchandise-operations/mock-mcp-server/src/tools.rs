use serde_json::{Value, json};

pub(crate) fn definitions() -> Value {
    json!([
        {
            "name": "check_idempotency",
            "description": "查询 ABAR 写 Tool 的幂等结果，仅供宿主恢复流程使用。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotencyKey"],
                "properties": {
                    "idempotencyKey": {"type": "string"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "create_transfer_order",
            "description": "根据已批准且已冻结的调拨方案创建幂等模拟调拨单。此工具会写入业务状态。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["planDigest", "requiredScopes"],
                "properties": {
                    "planDigest": {"type": "string"},
                    "requiredScopes": {"type": "array", "items": {"type": "string"}, "minItems": 1, "uniqueItems": true}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "calculate_allocation_plan",
            "description": "使用中心仓库存和门店销售数据生成首铺配货计划。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "regions": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 5, "uniqueItems": true},
                    "planningWindowDays": {"type": "integer", "minimum": 1, "maximum": 28},
                    "skuId": {"type": "string"},
                    "maxRecommendations": {"type": "integer", "minimum": 1, "maximum": 20},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "create_allocation_order",
            "description": "根据已批准且已冻结的配货计划创建幂等配货单。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["planDigest", "requiredScopes"],
                "properties": {
                    "planDigest": {"type": "string"},
                    "requiredScopes": {"type": "array", "items": {"type": "string"}, "minItems": 1, "uniqueItems": true}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "calculate_replenishment_plan",
            "description": "使用中心仓库存、目标覆盖天数和门店销量生成补货计划。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "regions": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 5, "uniqueItems": true},
                    "planningWindowDays": {"type": "integer", "minimum": 1, "maximum": 28},
                    "skuId": {"type": "string"},
                    "maxRecommendations": {"type": "integer", "minimum": 1, "maximum": 20},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "create_replenishment_order",
            "description": "根据已批准且已冻结的补货计划创建幂等补货单。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["planDigest", "requiredScopes"],
                "properties": {
                    "planDigest": {"type": "string"},
                    "requiredScopes": {"type": "array", "items": {"type": "string"}, "minItems": 1, "uniqueItems": true}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "calculate_transfer_recommendations",
            "description": "使用固定规则和模拟门店数据生成只读、可解释的门店间调拨建议。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "regions": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1,
                        "maxItems": 5,
                        "uniqueItems": true
                    },
                    "planningWindowDays": {"type": "integer", "minimum": 1, "maximum": 28},
                    "skuId": {"type": "string"},
                    "maxRecommendations": {"type": "integer", "minimum": 1, "maximum": 20},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "query_in_transit",
            "description": "查询模拟门店 SKU 的调入在途和待调出数量。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "city": {"type": "string"},
                    "storeId": {"type": "string"},
                    "skuId": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "query_sales_velocity",
            "description": "查询模拟门店 SKU 的 7 日、28 日销量和日均销售速度。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "city": {"type": "string"},
                    "storeId": {"type": "string"},
                    "skuId": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        },
        {
            "name": "query_store_inventory",
            "description": "查询模拟门店 SKU 的现存量、占用、可用量、覆盖天数和收货能力。",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "region": {"type": "string"},
                    "city": {"type": "string"},
                    "storeId": {"type": "string"},
                    "skuId": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                    "refreshSnapshot": {"type": "boolean"}
                }
            },
            "outputSchema": {"type": "object"}
        }
    ])
}
