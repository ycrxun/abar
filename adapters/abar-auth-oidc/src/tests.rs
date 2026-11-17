use crate::{OidcUserInfoAuthProvider, OidcUserInfoConfig, RoleApprovalAuthorizer};
use abar_api::message::PendingToolCall;
use abar_api::policy::{ApprovalAuthorizer, AuthContext, AuthProvider};
use abar_api::session::{Session, SessionStatus};
use axum::Json;
use axum::routing::get;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn maps_user_info_claims_and_enforces_role() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/userinfo",
        get(|| async {
            Json(json!({
                "sub": "alice",
                "tenant": "acme",
                "roles": ["approver"]
            }))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = OidcUserInfoAuthProvider::new(OidcUserInfoConfig {
        user_info_endpoint: format!("http://{address}/userinfo"),
        subject_claim: "sub".to_owned(),
        tenant_claim: "tenant".to_owned(),
        roles_claim: "roles".to_owned(),
        allow_http: true,
        timeout_seconds: 5,
        issuer: None,
        audience: None,
        introspection_endpoint: None,
        client_id: None,
        client_secret: None,
        cache_seconds: 30,
    })
    .unwrap();
    let auth = provider.authenticate("token").await.unwrap();
    assert_eq!(auth.subject_id, "alice");
    assert_eq!(auth.tenant.tenant_id, "acme");
    assert_eq!(auth.roles, vec!["approver"]);

    let session = Session {
        id: "ses-1".to_owned(),
        tenant: "acme".to_owned(),
        agent: "example-assistant".to_owned(),
        user: "bob".to_owned(),
        status: SessionStatus::Active,
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_active_at: "2026-11-17T00:00:00Z".to_owned(),
    };
    let pending = PendingToolCall {
        approval_id: "approval-1".to_owned(),
        tool_id: "records.write".to_owned(),
        arguments_json: "{}".to_owned(),
        idempotency_key: "key-1".to_owned(),
    };
    let authorizer = RoleApprovalAuthorizer::new("approver");
    assert!(
        authorizer
            .can_review(&auth, &session, &pending)
            .await
            .unwrap()
    );
    authorizer
        .authorize(
            &auth,
            &session,
            &pending,
            "approved",
            &["scope-a".to_owned()],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn enforces_introspection_issuer_audience_and_expiry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    let userinfo_calls = Arc::new(AtomicUsize::new(0));
    let introspection_calls = Arc::new(AtomicUsize::new(0));
    let userinfo_calls_handler = userinfo_calls.clone();
    let app = axum::Router::new()
        .route(
            "/userinfo",
            get(move || {
                let calls = userinfo_calls_handler.clone();
                async move {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Json(json!({
                        "sub": "alice",
                        "tenant": "acme",
                        "roles": ["approver"]
                    }))
                }
            }),
        )
        .route(
            "/introspect",
            axum::routing::post({
                let calls = introspection_calls.clone();
                move || {
                    let calls = calls.clone();
                    async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Json(json!({
                            "active": true,
                            "iss": "https://id.example.test",
                            "aud": "abar-api",
                            "exp": expires_at
                        }))
                    }
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let config = OidcUserInfoConfig {
        user_info_endpoint: format!("http://{address}/userinfo"),
        subject_claim: "sub".to_owned(),
        tenant_claim: "tenant".to_owned(),
        roles_claim: "roles".to_owned(),
        allow_http: true,
        timeout_seconds: 5,
        issuer: Some("https://id.example.test".to_owned()),
        audience: Some("abar-api".to_owned()),
        introspection_endpoint: Some(format!("http://{address}/introspect")),
        client_id: Some("abar-server".to_owned()),
        client_secret: Some("test-secret".to_owned()),
        cache_seconds: 30,
    };
    let provider = OidcUserInfoAuthProvider::new(config.clone()).unwrap();
    for _ in 0..2 {
        assert_eq!(
            provider.authenticate("token").await.unwrap().subject_id,
            "alice"
        );
    }
    assert_eq!(introspection_calls.load(Ordering::Relaxed), 1);
    assert_eq!(userinfo_calls.load(Ordering::Relaxed), 1);

    let wrong_audience = OidcUserInfoAuthProvider::new(OidcUserInfoConfig {
        audience: Some("other-api".to_owned()),
        ..config
    })
    .unwrap();
    assert!(wrong_audience.authenticate("token").await.is_err());
}

#[tokio::test]
async fn scoped_approver_only_sees_and_decides_allowed_scopes() {
    let session = Session {
        id: "ses-1".to_owned(),
        tenant: "acme".to_owned(),
        agent: "example-assistant".to_owned(),
        user: "requester".to_owned(),
        status: SessionStatus::Active,
        created_at: "2026-11-17T00:00:00Z".to_owned(),
        last_active_at: "2026-11-17T00:00:00Z".to_owned(),
    };
    let first = PendingToolCall {
        approval_id: "approval-first".to_owned(),
        tool_id: "records.write".to_owned(),
        arguments_json: r#"{"requiredScopes":["scope-a"]}"#.to_owned(),
        idempotency_key: "key-first".to_owned(),
    };
    let second = PendingToolCall {
        approval_id: "approval-second".to_owned(),
        tool_id: "records.write".to_owned(),
        arguments_json: r#"{"requiredScopes":["scope-b"]}"#.to_owned(),
        idempotency_key: "key-second".to_owned(),
    };
    let auth = AuthContext {
        roles: vec!["approver:scope-a".to_owned()],
        ..AuthContext::new("alice", abar_api::context::TenantContext::new("acme"))
    };
    let authorizer = RoleApprovalAuthorizer::with_scope_field("approver", "requiredScopes");
    assert!(
        authorizer
            .can_review(&auth, &session, &first)
            .await
            .unwrap()
    );
    assert!(
        !authorizer
            .can_review(&auth, &session, &second)
            .await
            .unwrap()
    );
    authorizer
        .authorize(&auth, &session, &first, "approved", &["scope-a".to_owned()])
        .await
        .unwrap();
    assert!(
        authorizer
            .authorize(
                &auth,
                &session,
                &second,
                "approved",
                &["scope-b".to_owned()]
            )
            .await
            .is_err()
    );
}
