use super::catalog::MockCatalog;
use super::data::{DATA_AS_OF, RULES_VERSION};
use super::orders::{PlanKind, StoredPlan};
use super::util::{ceil_to_u32, round_one, sha256_json};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl MockCatalog {
    pub(super) fn recommendations(&self, arguments: &Value) -> Result<Value, String> {
        self.plan(arguments, PlanKind::Transfer)
    }

    pub(super) fn allocation_plan(&self, arguments: &Value) -> Result<Value, String> {
        self.plan(arguments, PlanKind::Allocation)
    }

    pub(super) fn replenishment_plan(&self, arguments: &Value) -> Result<Value, String> {
        self.plan(arguments, PlanKind::Replenishment)
    }

    fn plan(&self, arguments: &Value, kind: PlanKind) -> Result<Value, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Arguments {
            region: Option<String>,
            regions: Option<Vec<String>>,
            planning_window_days: Option<u32>,
            sku_id: Option<String>,
            max_recommendations: Option<usize>,
        }

        let arguments: Arguments = serde_json::from_value(arguments.clone())
            .map_err(|error| format!("invalid arguments: {error}"))?;
        let regions = arguments.regions.unwrap_or_else(|| {
            vec![
                arguments
                    .region
                    .clone()
                    .unwrap_or_else(|| "华东".to_owned()),
            ]
        });
        if regions.is_empty() {
            return Err("regions must not be empty".to_owned());
        }
        let known_regions: BTreeSet<_> = self
            .stores
            .iter()
            .map(|store| store.region.as_str())
            .collect();
        if let Some(unknown) = regions
            .iter()
            .find(|region| !known_regions.contains(region.as_str()))
        {
            return Err(format!("unknown region: {unknown}"));
        }
        let planning_window = arguments.planning_window_days.unwrap_or(7).clamp(1, 28);
        let maximum = arguments.max_recommendations.unwrap_or(6).clamp(1, 20);
        let store_indexes: Vec<_> = self
            .stores
            .iter()
            .enumerate()
            .filter(|(_, store)| regions.contains(&store.region))
            .map(|(index, _)| index)
            .collect();
        let selected_store: BTreeSet<_> = store_indexes.iter().copied().collect();
        let mut candidates = Vec::new();
        for sku_index in 0..self.skus.len() {
            if arguments
                .sku_id
                .as_deref()
                .is_some_and(|sku| self.skus[sku_index].id != sku)
            {
                continue;
            }
            let mut warehouse_remaining = self.warehouse_stock[sku_index];
            for &target_index in &store_indexes {
                let target = self.record(target_index, sku_index);
                let target_daily = target.average_daily_sales();
                if target_daily == 0.0 || target.cover_days() >= f64::from(planning_window) {
                    continue;
                }
                let desired = ceil_to_u32(target_daily * f64::from(planning_window) * 1.2)
                    .saturating_sub(target.available())
                    .saturating_sub(target.inbound_7d);
                if desired == 0 {
                    continue;
                }
                if kind != PlanKind::Transfer {
                    let quantity = desired
                        .min(warehouse_remaining)
                        .min(target.receiving_capacity)
                        .min(20);
                    if quantity < 2 {
                        continue;
                    }
                    warehouse_remaining = warehouse_remaining.saturating_sub(quantity);
                    let projected_available = target.available() + target.inbound_7d + quantity;
                    candidates.push(json!({
                        "sourceStoreId": format!("WH-{}", self.stores[target_index].region),
                        "sourceStoreName": format!("{}中心仓", self.stores[target_index].region),
                        "sourceCity": self.stores[target_index].city,
                        "targetStoreId": self.stores[target_index].id,
                        "targetStoreName": self.stores[target_index].name,
                        "targetCity": self.stores[target_index].city,
                        "skuId": self.skus[sku_index].id,
                        "skuName": self.skus[sku_index].name,
                        "category": self.skus[sku_index].category,
                        "size": self.skus[sku_index].size,
                        "color": self.skus[sku_index].color,
                        "quantity": quantity,
                        "targetAvailableBefore": target.available(),
                        "targetInboundWithin7d": target.inbound_7d,
                        "targetCoverDaysBefore": round_one(target.cover_days()),
                        "targetCoverDaysAfter": round_one(
                            f64::from(projected_available) / target_daily
                        ),
                        "sourceAvailableAfterSafety": warehouse_remaining,
                        "sourceCoverDays": 999.0,
                        "sameCity": true,
                        "reasonCodes": [
                            "TARGET_COVER_BELOW_PLANNING_WINDOW",
                            "CENTRAL_WAREHOUSE_AVAILABLE"
                        ],
                        "priority": "HIGH"
                    }));
                    continue;
                }
                let source = store_indexes
                    .iter()
                    .copied()
                    .filter(|&source_index| source_index != target_index)
                    .filter(|source_index| selected_store.contains(source_index))
                    .filter_map(|source_index| {
                        let source = self.record(source_index, sku_index);
                        let safety = ceil_to_u32(source.average_daily_sales() * 14.0).max(4);
                        let transferable = source.available().saturating_sub(safety);
                        (source.cover_days() > f64::from(planning_window * 3) && transferable >= 2)
                            .then_some((source_index, source, transferable))
                    })
                    .max_by(
                        |(left_index, left, left_transferable),
                         (right_index, right, right_transferable)| {
                            let left_same_city =
                                self.stores[*left_index].city == self.stores[target_index].city;
                            let right_same_city =
                                self.stores[*right_index].city == self.stores[target_index].city;
                            left_same_city
                                .cmp(&right_same_city)
                                .then_with(|| left.cover_days().total_cmp(&right.cover_days()))
                                .then_with(|| left_transferable.cmp(right_transferable))
                        },
                    );
                let Some((source_index, source, transferable)) = source else {
                    continue;
                };
                let quantity = desired
                    .min(transferable)
                    .min(target.receiving_capacity)
                    .min(12);
                if quantity < 2 {
                    continue;
                }
                let projected_available = target.available() + target.inbound_7d + quantity;
                let projected_cover = if target_daily == 0.0 {
                    999.0
                } else {
                    f64::from(projected_available) / target_daily
                };
                candidates.push(json!({
                    "sourceStoreId": self.stores[source_index].id,
                    "sourceStoreName": self.stores[source_index].name,
                    "sourceCity": self.stores[source_index].city,
                    "targetStoreId": self.stores[target_index].id,
                    "targetStoreName": self.stores[target_index].name,
                    "targetCity": self.stores[target_index].city,
                    "skuId": self.skus[sku_index].id,
                    "skuName": self.skus[sku_index].name,
                    "category": self.skus[sku_index].category,
                    "size": self.skus[sku_index].size,
                    "color": self.skus[sku_index].color,
                    "quantity": quantity,
                    "targetAvailableBefore": target.available(),
                    "targetInboundWithin7d": target.inbound_7d,
                    "targetCoverDaysBefore": round_one(target.cover_days()),
                    "targetCoverDaysAfter": round_one(projected_cover),
                    "sourceAvailableAfterSafety": transferable,
                    "sourceCoverDays": round_one(source.cover_days()),
                    "sameCity": self.stores[source_index].city == self.stores[target_index].city,
                    "reasonCodes": [
                        "TARGET_COVER_BELOW_PLANNING_WINDOW",
                        "SOURCE_EXCESS_ABOVE_SAFETY_STOCK"
                    ],
                    "priority": if self.stores[source_index].city == self.stores[target_index].city {
                        "HIGH"
                    } else {
                        "NORMAL"
                    }
                }));
            }
        }
        candidates.sort_by(|left, right| {
            priority_rank(left)
                .cmp(&priority_rank(right))
                .then_with(|| cover_before(left).total_cmp(&cover_before(right)))
                .then_with(|| {
                    left["targetStoreId"]
                        .as_str()
                        .cmp(&right["targetStoreId"].as_str())
                })
                .then_with(|| left["skuId"].as_str().cmp(&right["skuId"].as_str()))
        });
        let matched = candidates.len();
        let mut target_counts = BTreeMap::<String, usize>::new();
        let recommendations: Vec<_> = candidates
            .into_iter()
            .filter(|candidate| {
                let target = candidate["targetStoreId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let count = target_counts.entry(target).or_default();
                if *count >= 2 {
                    return false;
                }
                *count += 1;
                true
            })
            .take(maximum)
            .collect();
        let line_count = recommendations.len();
        let plan_type = match kind {
            PlanKind::Allocation => "allocation",
            PlanKind::Replenishment => "replenishment",
            PlanKind::Transfer => "transfer",
        };
        let business_label = match kind {
            PlanKind::Allocation => "配货",
            PlanKind::Replenishment => "补货",
            PlanKind::Transfer => "调拨",
        };
        let mut result = json!({
            "planType": plan_type,
            "dataAsOf": DATA_AS_OF,
            "rulesVersion": RULES_VERSION,
            "planningWindowDays": planning_window,
            "region": if regions.len() == 1 {
                Value::String(regions[0].clone())
            } else {
                Value::Null
            },
            "regions": regions.clone(),
            "summary": {
                "evaluatedStores": store_indexes.len(),
                "evaluatedSkus": self.skus.len(),
                "matchedRecommendations": matched,
                "returnedRecommendations": recommendations.len(),
                "sameCityRecommendations": recommendations.iter().filter(|value| value["sameCity"] == true).count()
            },
            "recommendations": recommendations,
            "excluded": [
                {
                    "reasonCode": "SENSITIVE_FINANCIAL_DATA_EXCLUDED",
                    "description": "成本、毛利和供应商价格未进入本模拟输出。"
                },
                {
                    "reasonCode": "WRITE_OPERATIONS_NOT_EXECUTED",
                    "description": "本工具只生成建议，不创建调拨单。"
                }
            ],
            "disclaimer": format!("数据、门店和商品均为确定性模拟，不代表真实{business_label}结果。"),
            "requiredScopes": regions.clone()
        });
        let plan_digest = sha256_json(&result);
        result["planDigest"] = Value::String(plan_digest.clone());
        result["confirmation"] = json!({
            "planType": plan_type,
            "planDigest": plan_digest
        });
        self.plans.lock().expect("mock plan store poisoned").insert(
            plan_digest,
            StoredPlan {
                required_scopes: regions.clone(),
                line_count,
                kind,
            },
        );
        Ok(result)
    }
}

fn priority_rank(value: &Value) -> u8 {
    if value["priority"] == "HIGH" { 0 } else { 1 }
}

fn cover_before(value: &Value) -> f64 {
    value["targetCoverDaysBefore"].as_f64().unwrap_or(999.0)
}
