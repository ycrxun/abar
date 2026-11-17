use super::data::{InventoryRecord, Sku, Store, build_inventory, build_skus, build_stores};
use super::orders::{StoredOrder, StoredPlan};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Catalog of synthetic stores, SKUs, stock positions, plans, and orders.
#[derive(Clone, Debug)]
pub struct MockCatalog {
    pub(super) stores: Vec<Store>,
    pub(super) skus: Vec<Sku>,
    pub(super) inventory: Vec<InventoryRecord>,
    pub(super) warehouse_stock: Vec<u32>,
    pub(super) plans: Arc<Mutex<BTreeMap<String, StoredPlan>>>,
    pub(super) orders: Arc<Mutex<BTreeMap<String, StoredOrder>>>,
    pub(super) order_sequence: Arc<AtomicU64>,
    snapshots: Arc<Mutex<BTreeMap<String, u64>>>,
    snapshot_sequence: Arc<AtomicU64>,
}

impl Default for MockCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCatalog {
    /// Builds the fixed 300-store demo catalog.
    #[must_use]
    pub fn new() -> Self {
        let stores = build_stores();
        let skus = build_skus();
        let mut inventory = Vec::with_capacity(stores.len() * skus.len());
        for store_index in 0..stores.len() {
            for sku_index in 0..skus.len() {
                inventory.push(build_inventory(store_index, sku_index));
            }
        }
        let warehouse_stock = (0..skus.len())
            .map(|sku_index| 180 + u32::try_from(sku_index * 37).unwrap_or(0))
            .collect();
        Self {
            stores,
            skus,
            inventory,
            warehouse_stock,
            plans: Arc::new(Mutex::new(BTreeMap::new())),
            orders: Arc::new(Mutex::new(BTreeMap::new())),
            order_sequence: Arc::new(AtomicU64::new(0)),
            snapshots: Arc::new(Mutex::new(BTreeMap::new())),
            snapshot_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the MCP tools/list payload.
    #[must_use]
    pub fn tool_definitions(&self) -> Value {
        super::tools::definitions()
    }

    /// Executes one mock tool.
    pub fn call_tool(
        &self,
        name: &str,
        arguments: &Value,
        idempotency_key: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Value, String> {
        match name {
            "check_idempotency" => self.lookup_idempotency(arguments),
            "create_transfer_order" => {
                self.create_transfer_order(arguments, idempotency_key, metadata)
            }
            "create_allocation_order" => {
                self.create_allocation_order(arguments, idempotency_key, metadata)
            }
            "create_replenishment_order" => {
                self.create_replenishment_order(arguments, idempotency_key, metadata)
            }
            "calculate_allocation_plan" => self.allocation_plan(arguments),
            "calculate_replenishment_plan" => self.replenishment_plan(arguments),
            "calculate_transfer_recommendations" => self.recommendations(arguments),
            "query_store_inventory" => self.query_inventory(arguments),
            "query_sales_velocity" => self.query_sales(arguments),
            "query_in_transit" => self.query_transit(arguments),
            _ => Err(format!("unknown tool: {name}")),
        }
    }

    /// Number of synthetic stores.
    #[must_use]
    pub fn store_count(&self) -> usize {
        self.stores.len()
    }

    /// Returns a stable Session snapshot, or advances it when explicitly refreshed.
    #[must_use]
    pub fn snapshot(&self, session_id: &str, refresh: bool) -> (String, u64) {
        let mut snapshots = self.snapshots.lock().expect("snapshot store poisoned");
        let version = if refresh {
            let version = self.snapshot_sequence.fetch_add(1, Ordering::Relaxed) + 1;
            snapshots.insert(session_id.to_owned(), version);
            version
        } else {
            *snapshots
                .entry(session_id.to_owned())
                .or_insert_with(|| self.snapshot_sequence.fetch_add(1, Ordering::Relaxed) + 1)
        };
        (format!("snapshot-{version:06}"), version)
    }
}
