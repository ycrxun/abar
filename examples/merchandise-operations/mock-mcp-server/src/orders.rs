use super::catalog::MockCatalog;
use super::data::DATA_AS_OF;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

#[derive(Clone, Debug)]
pub(super) struct StoredPlan {
    pub(super) required_scopes: Vec<String>,
    pub(super) line_count: usize,
    pub(super) kind: PlanKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PlanKind {
    Allocation,
    Replenishment,
    Transfer,
}

impl PlanKind {
    fn order_prefix(self) -> &'static str {
        match self {
            Self::Allocation => "MOCK-AL",
            Self::Replenishment => "MOCK-RP",
            Self::Transfer => "MOCK-TR",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct StoredOrder {
    plan_digest: String,
    order_id: String,
    approval_id: String,
    line_count: usize,
}

impl MockCatalog {
    pub(super) fn create_transfer_order(
        &self,
        arguments: &Value,
        idempotency_key: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Value, String> {
        self.create_plan_order(PlanKind::Transfer, arguments, idempotency_key, metadata)
    }

    pub(super) fn create_allocation_order(
        &self,
        arguments: &Value,
        idempotency_key: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Value, String> {
        self.create_plan_order(PlanKind::Allocation, arguments, idempotency_key, metadata)
    }

    pub(super) fn create_replenishment_order(
        &self,
        arguments: &Value,
        idempotency_key: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Value, String> {
        self.create_plan_order(
            PlanKind::Replenishment,
            arguments,
            idempotency_key,
            metadata,
        )
    }

    fn create_plan_order(
        &self,
        kind: PlanKind,
        arguments: &Value,
        idempotency_key: Option<&str>,
        metadata: Option<&Value>,
    ) -> Result<Value, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Arguments {
            plan_digest: String,
            #[serde(default)]
            required_scopes: Vec<String>,
        }

        let arguments: Arguments = serde_json::from_value(arguments.clone())
            .map_err(|error| format!("invalid arguments: {error}"))?;
        let idempotency_key = idempotency_key
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "create_transfer_order requires Idempotency-Key".to_owned())?;
        let approval = metadata
            .and_then(|value| value.get("abarApproval"))
            .ok_or_else(|| "create_transfer_order requires approval evidence".to_owned())?;
        let approval_id = approval
            .get("approvalId")
            .and_then(Value::as_str)
            .ok_or_else(|| "approval evidence has no approvalId".to_owned())?;
        let scopes: BTreeSet<_> = approval
            .get("scopes")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let plan = self
            .plans
            .lock()
            .expect("mock plan store poisoned")
            .get(&arguments.plan_digest)
            .cloned()
            .ok_or_else(|| format!("unknown plan digest: {}", arguments.plan_digest))?;
        if plan.kind != kind {
            return Err("plan type does not match order type".to_owned());
        }
        if arguments.required_scopes.is_empty()
            || arguments.required_scopes.iter().collect::<BTreeSet<_>>()
                != plan.required_scopes.iter().collect::<BTreeSet<_>>()
        {
            return Err("order requiredScopes do not match plan scopes".to_owned());
        }
        let missing_scopes: Vec<_> = plan
            .required_scopes
            .iter()
            .filter(|scope| !scopes.contains(scope.as_str()))
            .cloned()
            .collect();
        if !missing_scopes.is_empty() {
            return Err(format!(
                "approval does not cover required scopes: {}",
                missing_scopes.join(",")
            ));
        }
        let mut orders = self.orders.lock().expect("mock order store poisoned");
        if let Some(existing) = orders.get(idempotency_key) {
            if existing.plan_digest != arguments.plan_digest {
                return Err("idempotency key is already bound to another plan".to_owned());
            }
            return Ok(json!({
                "orderId": existing.order_id,
                "planDigest": existing.plan_digest,
                "status": "CREATED",
                "approvalId": approval_id,
                "originalApprovalId": existing.approval_id,
                "lineCount": existing.line_count,
                "reused": true,
                "createdAt": DATA_AS_OF
            }));
        }
        let sequence = self.order_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let order = StoredOrder {
            plan_digest: arguments.plan_digest,
            order_id: format!("{}-20261116-{sequence:04}", kind.order_prefix()),
            approval_id: approval_id.to_owned(),
            line_count: plan.line_count,
        };
        let response = json!({
            "orderId": order.order_id,
            "planDigest": order.plan_digest,
            "status": "CREATED",
            "approvalId": order.approval_id,
            "lineCount": order.line_count,
            "reused": false,
            "createdAt": DATA_AS_OF
        });
        orders.insert(idempotency_key.to_owned(), order);
        if let Ok(delay_ms) = std::env::var("ABAR_MOCK_AFTER_WRITE_DELAY_MS")
            && let Ok(delay_ms) = delay_ms.parse::<u64>()
        {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        Ok(response)
    }

    pub(super) fn lookup_idempotency(&self, arguments: &Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Arguments {
            idempotency_key: String,
        }

        let arguments: Arguments = serde_json::from_value(arguments.clone())
            .map_err(|error| format!("invalid arguments: {error}"))?;
        let orders = self.orders.lock().expect("mock order store poisoned");
        let Some(order) = orders.get(&arguments.idempotency_key) else {
            return Ok(json!({"found": false, "result": null}));
        };
        Ok(json!({
            "found": true,
            "result": {
                "orderId": order.order_id,
                "planDigest": order.plan_digest,
                "status": "CREATED",
                "approvalId": order.approval_id,
                "lineCount": order.line_count,
                "reused": true,
                "createdAt": DATA_AS_OF
            }
        }))
    }

    pub fn order_count(&self) -> usize {
        self.orders.lock().expect("mock order store poisoned").len()
    }
}
