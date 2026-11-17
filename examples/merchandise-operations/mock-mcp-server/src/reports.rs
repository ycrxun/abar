use super::catalog::MockCatalog;
use super::data::DATA_AS_OF;
use super::util::{round_one, round_two};
use serde_json::{Value, json};

impl MockCatalog {
    /// Builds the manager-only merchandise anomaly report.
    pub fn merchandise_anomalies(&self, limit: usize) -> Value {
        let limit = limit.clamp(1, 20);
        let mut sales = self
            .inventory
            .iter()
            .map(|record| {
                let baseline_daily = f64::from(record.sales_28d) / 28.0;
                let current_daily = record.average_daily_sales();
                let change = if baseline_daily == 0.0 {
                    0.0
                } else {
                    (current_daily - baseline_daily) / baseline_daily * 100.0
                };
                anomaly_item(
                    self,
                    record.store_index,
                    record.sku_index,
                    json!({
                        "changePercent": round_two(change),
                        "currentSales7d": record.sales_7d
                    }),
                )
            })
            .collect::<Vec<_>>();
        sales.sort_by(|left, right| {
            left["changePercent"]
                .as_f64()
                .unwrap_or(f64::MAX)
                .total_cmp(&right["changePercent"].as_f64().unwrap_or(f64::MAX))
        });
        sales.truncate(limit);
        rank(&mut sales);

        let mut margin = self
            .inventory
            .iter()
            .map(|record| {
                let severity = 5
                    + (record.store_index * 17 + record.sku_index * 31 + record.sales_7d as usize)
                        % 90;
                anomaly_item(
                    self,
                    record.store_index,
                    record.sku_index,
                    json!({
                        "changeBps": -(severity as i64),
                        "managerOnly": true
                    }),
                )
            })
            .collect::<Vec<_>>();
        margin.sort_by_key(|item| item["changeBps"].as_i64().unwrap_or(0));
        margin.truncate(limit);
        rank(&mut margin);

        let mut shortage = self
            .inventory
            .iter()
            .filter(|record| record.available() < 5)
            .map(|record| {
                anomaly_item(
                    self,
                    record.store_index,
                    record.sku_index,
                    json!({
                        "available": record.available(),
                        "sales7d": record.sales_7d,
                        "coverDays": round_one(record.cover_days())
                    }),
                )
            })
            .collect::<Vec<_>>();
        shortage.sort_by(|left, right| {
            left["coverDays"]
                .as_f64()
                .unwrap_or(f64::MAX)
                .total_cmp(&right["coverDays"].as_f64().unwrap_or(f64::MAX))
        });
        shortage.truncate(limit);
        rank(&mut shortage);

        json!({
            "reportType": "merchandise-anomalies/v1",
            "dataAsOf": DATA_AS_OF,
            "generatedAt": DATA_AS_OF,
            "managerOnly": true,
            "buckets": {
                "salesDecline": sales,
                "grossMarginDecline": margin,
                "inventoryShortage": shortage
            }
        })
    }
}

fn anomaly_item(
    catalog: &MockCatalog,
    store_index: usize,
    sku_index: usize,
    mut metrics: Value,
) -> Value {
    let store = &catalog.stores[store_index];
    let sku = &catalog.skus[sku_index];
    metrics["storeId"] = json!(store.id);
    metrics["storeName"] = json!(store.name);
    metrics["region"] = json!(store.region);
    metrics["skuId"] = json!(sku.id);
    metrics["skuName"] = json!(sku.name);
    metrics
}

fn rank(items: &mut [Value]) {
    for (index, item) in items.iter_mut().enumerate() {
        item["rank"] = json!(index + 1);
    }
}
