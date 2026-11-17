use super::MockCatalog;
use serde_json::json;

#[test]
fn builds_realistic_catalog_shape() {
    let catalog = MockCatalog::new();
    assert_eq!(catalog.store_count(), 300);
    let result = catalog
        .call_tool(
            "query_store_inventory",
            &json!({"region": "华东", "limit": 5}),
            None,
            None,
        )
        .unwrap();
    assert_eq!(result["matchedTotal"], 60 * 24);
    assert_eq!(result["returned"], 5);
    assert_eq!(result["dataAsOf"], "2026-11-16T09:30:00+08:00");
}

#[test]
fn generates_explainable_transfer_recommendations() {
    let catalog = MockCatalog::new();
    let result = catalog
        .call_tool(
            "calculate_transfer_recommendations",
            &json!({"region": "华东", "maxRecommendations": 6}),
            None,
            None,
        )
        .unwrap();
    let recommendations = result["recommendations"].as_array().unwrap();
    assert!(!recommendations.is_empty());
    assert!(recommendations.len() <= 6);
    assert!(recommendations.iter().all(|item| {
        item["quantity"]
            .as_u64()
            .is_some_and(|quantity| quantity >= 2)
            && item.get("reasonCodes").is_some()
            && item.get("grossMargin").is_none()
            && item.get("cost").is_none()
    }));
}

#[test]
fn filters_by_store_and_sku() {
    let catalog = MockCatalog::new();
    let result = catalog
        .call_tool(
            "query_sales_velocity",
            &json!({"storeId": "ST001", "skuId": "AW-COAT-001"}),
            None,
            None,
        )
        .unwrap();
    assert_eq!(result["matchedTotal"], 1);
    assert_eq!(result["items"][0]["storeId"], "ST001");
    assert_eq!(result["items"][0]["skuId"], "AW-COAT-001");
}

#[test]
fn creates_one_order_per_idempotency_key_after_scope_approval() {
    let catalog = MockCatalog::new();
    let plan = catalog
        .call_tool(
            "calculate_transfer_recommendations",
            &json!({"region": "华东", "maxRecommendations": 3}),
            None,
            None,
        )
        .unwrap();
    let plan_digest = plan["planDigest"].as_str().unwrap();
    let arguments = json!({
        "planDigest": plan_digest,
        "requiredScopes": plan["requiredScopes"].clone()
    });

    assert!(
        catalog
            .call_tool("create_transfer_order", &arguments, Some("key-1"), None)
            .is_err()
    );
    let approval_metadata = json!({
        "abarApproval": {
            "approvalId": "approval-1",
            "actor": "east-region",
            "scopes": ["华东"]
        }
    });
    let first = catalog
        .call_tool(
            "create_transfer_order",
            &arguments,
            Some("key-1"),
            Some(&approval_metadata),
        )
        .unwrap();
    let second = catalog
        .call_tool(
            "create_transfer_order",
            &arguments,
            Some("key-1"),
            Some(&approval_metadata),
        )
        .unwrap();

    assert_eq!(first["orderId"], second["orderId"]);
    assert_eq!(first["reused"], false);
    assert_eq!(second["reused"], true);
    assert_eq!(first["approvalId"], "approval-1");
}

#[test]
fn rejects_approval_without_required_region_scope() {
    let catalog = MockCatalog::new();
    let plan = catalog
        .call_tool(
            "calculate_transfer_recommendations",
            &json!({"region": "华南", "maxRecommendations": 2}),
            None,
            None,
        )
        .unwrap();
    let arguments = json!({
        "planDigest": plan["planDigest"],
        "requiredScopes": plan["requiredScopes"].clone()
    });
    let approval_metadata = json!({
        "abarApproval": {
            "approvalId": "approval-2",
            "actor": "east-region",
            "scopes": ["华东"]
        }
    });

    let error = catalog
        .call_tool(
            "create_transfer_order",
            &arguments,
            Some("key-2"),
            Some(&approval_metadata),
        )
        .unwrap_err();

    assert!(error.contains("华南"));
}

#[test]
fn cross_region_plan_requires_every_scope() {
    let catalog = MockCatalog::new();
    let plan = catalog
        .call_tool(
            "calculate_transfer_recommendations",
            &json!({"regions": ["华东", "华南"], "maxRecommendations": 2}),
            None,
            None,
        )
        .unwrap();
    assert_eq!(plan["requiredScopes"], json!(["华东", "华南"]));
    let arguments = json!({
        "planDigest": plan["planDigest"],
        "requiredScopes": plan["requiredScopes"].clone()
    });
    let partial_approval = json!({
        "abarApproval": {
            "approvalId": "approval-partial",
            "actor": "east-region",
            "scopes": ["华东"]
        }
    });
    let error = catalog
        .call_tool(
            "create_transfer_order",
            &arguments,
            Some("key-partial"),
            Some(&partial_approval),
        )
        .unwrap_err();
    assert!(error.contains("华南"));

    let complete_approval = json!({
        "abarApproval": {
            "approvalId": "approval-complete",
            "actor": "joint-regional-approval",
            "scopes": ["华东", "华南"]
        }
    });
    assert!(
        catalog
            .call_tool(
                "create_transfer_order",
                &arguments,
                Some("key-complete"),
                Some(&complete_approval),
            )
            .is_ok()
    );
}
