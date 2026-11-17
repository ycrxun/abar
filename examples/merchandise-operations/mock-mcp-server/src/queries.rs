use super::catalog::MockCatalog;
use super::data::{DATA_AS_OF, InventoryRecord, Sku, Store};
use super::util::{round_one, round_two};
use serde::Deserialize;
use serde_json::{Value, json};

impl MockCatalog {
    pub(super) fn query_inventory(&self, arguments: &Value) -> Result<Value, String> {
        let query: Query = parse_query(arguments)?;
        let records = self.filtered_records(&query);
        let total = records.len();
        let items: Vec<_> = records
            .into_iter()
            .take(query.limit())
            .map(|(store_index, sku_index, record)| {
                let store = &self.stores[store_index];
                let sku = &self.skus[sku_index];
                json!({
                    "storeId": store.id,
                    "storeName": store.name,
                    "region": store.region,
                    "city": store.city,
                    "skuId": sku.id,
                    "skuName": sku.name,
                    "category": sku.category,
                    "size": sku.size,
                    "color": sku.color,
                    "onHand": record.on_hand,
                    "reserved": record.reserved,
                    "available": record.available(),
                    "sales7d": record.sales_7d,
                    "inboundWithin7d": record.inbound_7d,
                    "outboundPending": record.outbound_pending,
                    "coverDays": round_one(record.cover_days()),
                    "receivingCapacity": record.receiving_capacity
                })
            })
            .collect();
        Ok(json!({
            "dataAsOf": DATA_AS_OF,
            "matchedTotal": total,
            "returned": items.len(),
            "items": items
        }))
    }

    pub(super) fn query_sales(&self, arguments: &Value) -> Result<Value, String> {
        let query: Query = parse_query(arguments)?;
        let records = self.filtered_records(&query);
        let total = records.len();
        let items: Vec<_> = records
            .into_iter()
            .take(query.limit())
            .map(|(store_index, sku_index, record)| {
                let store = &self.stores[store_index];
                let sku = &self.skus[sku_index];
                json!({
                    "storeId": store.id,
                    "storeName": store.name,
                    "region": store.region,
                    "city": store.city,
                    "skuId": sku.id,
                    "skuName": sku.name,
                    "sales7d": record.sales_7d,
                    "sales28d": record.sales_28d,
                    "averageDailySales7d": round_two(record.average_daily_sales()),
                    "available": record.available(),
                    "coverDays": round_one(record.cover_days())
                })
            })
            .collect();
        Ok(json!({
            "dataAsOf": DATA_AS_OF,
            "matchedTotal": total,
            "returned": items.len(),
            "items": items
        }))
    }

    pub(super) fn query_transit(&self, arguments: &Value) -> Result<Value, String> {
        let query: Query = parse_query(arguments)?;
        let records: Vec<_> = self
            .filtered_records(&query)
            .into_iter()
            .filter(|(_, _, record)| record.inbound_7d > 0 || record.outbound_pending > 0)
            .collect();
        let total = records.len();
        let items: Vec<_> = records
            .into_iter()
            .take(query.limit())
            .map(|(store_index, sku_index, record)| {
                let store = &self.stores[store_index];
                let sku = &self.skus[sku_index];
                json!({
                    "storeId": store.id,
                    "storeName": store.name,
                    "region": store.region,
                    "city": store.city,
                    "skuId": sku.id,
                    "skuName": sku.name,
                    "inboundWithin7d": record.inbound_7d,
                    "outboundPending": record.outbound_pending,
                    "expectedInboundWindow": if record.inbound_7d > 0 {
                        "2026-11-17/2026-11-22"
                    } else {
                        "NONE"
                    },
                    "status": "SIMULATED_IN_TRANSIT"
                })
            })
            .collect();
        Ok(json!({
            "dataAsOf": DATA_AS_OF,
            "matchedTotal": total,
            "returned": items.len(),
            "items": items
        }))
    }

    fn filtered_records(&self, query: &Query) -> Vec<(usize, usize, InventoryRecord)> {
        self.inventory
            .iter()
            .filter_map(|record| {
                let store = &self.stores[record.store_index];
                let sku = &self.skus[record.sku_index];
                query
                    .matches(store, sku)
                    .then(|| (record.store_index, record.sku_index, record.clone()))
            })
            .collect()
    }

    pub(super) fn record(&self, store_index: usize, sku_index: usize) -> InventoryRecord {
        self.inventory[store_index * self.skus.len() + sku_index].clone()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Query {
    region: Option<String>,
    city: Option<String>,
    store_id: Option<String>,
    sku_id: Option<String>,
    limit: Option<usize>,
}

impl Query {
    fn matches(&self, store: &Store, sku: &Sku) -> bool {
        self.region
            .as_ref()
            .is_none_or(|value| store.region == *value)
            && self.city.as_ref().is_none_or(|value| store.city == *value)
            && self
                .store_id
                .as_ref()
                .is_none_or(|value| store.id == *value)
            && self.sku_id.as_ref().is_none_or(|value| sku.id == *value)
    }

    fn limit(&self) -> usize {
        self.limit.unwrap_or(50).clamp(1, 200)
    }
}

fn parse_query(arguments: &Value) -> Result<Query, String> {
    serde_json::from_value(arguments.clone()).map_err(|error| format!("invalid arguments: {error}"))
}
