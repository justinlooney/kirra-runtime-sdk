// src/gateway/policy_layer.rs
//
// Tower Layer/Service that enforces the Aegis posture-aware command routing
// policy on every inbound HTTP request before it reaches the inner service.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Body;
use http::{Request, Response, StatusCode};
use tower::{Layer, Service};

use crate::gateway::interceptor::{now_ms, should_route_command, SharedPostureCache};
use crate::gateway::policy::classify_http_command;

// ---------------------------------------------------------------------------
// Layer
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AegisPolicyLayer {
    pub cache: SharedPostureCache,
}

impl<S> Layer<S> for AegisPolicyLayer {
    type Service = AegisPolicyService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AegisPolicyService { inner, cache: self.cache.clone() }
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AegisPolicyService<S> {
    inner: S,
    cache: SharedPostureCache,
}

impl<S> Service<Request<Body>> for AegisPolicyService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let method = req.method().as_str().to_string();
        let path = req.uri().path().to_string();
        let command = classify_http_command(&method, &path);

        // Read the cache inside a scoped block so the RwLock guard is dropped
        // before the async boundary — holding a lock across an await is unsound.
        let allowed = {
            let now = now_ms();
            match self.cache.read() {
                Ok(guard) => match guard.as_ref() {
                    Some(posture) => should_route_command(posture, now, command),
                    None => false,
                },
                Err(_) => false,
            }
        };

        if !allowed {
            return Box::pin(async move {
                Ok(Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::from("AEGIS_POLICY_DENY"))
                    .unwrap())
            });
        }

        let mut inner = self.inner.clone();
        Box::pin(async move { inner.call(req).await })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::interceptor::{CachedFleetPosture, FleetPosture, NodeTrustState};
    use axum::{routing::get, routing::post, Router};
    use std::sync::{Arc, RwLock};
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str { "ok" }

    fn cache_with(posture: FleetPosture) -> SharedPostureCache {
        Arc::new(RwLock::new(Some(CachedFleetPosture {
            node_id: "test-node".to_string(),
            local_status: NodeTrustState::Trusted,
            propagated_status: posture,
            blocked_by: vec![],
            updated_at_epoch_ms: now_ms(),
        })))
    }

    #[tokio::test]
    async fn test_nominal_allows_write_state() {
        let app = Router::new()
            .route("/cmd_vel", post(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::Nominal) });

        let res = app
            .oneshot(Request::builder().method("POST").uri("/cmd_vel").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_nominal_allows_system_mutation() {
        let app = Router::new()
            .route("/reboot", post(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::Nominal) });

        let res = app
            .oneshot(Request::builder().method("POST").uri("/reboot").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_degraded_allows_read_telemetry() {
        let app = Router::new()
            .route("/telemetry/status", get(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::Degraded) });

        let res = app
            .oneshot(Request::builder().method("GET").uri("/telemetry/status").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_degraded_blocks_write_state() {
        let app = Router::new()
            .route("/cmd_vel", post(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::Degraded) });

        let res = app
            .oneshot(Request::builder().method("POST").uri("/cmd_vel").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_degraded_blocks_system_mutation() {
        let app = Router::new()
            .route("/reboot", post(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::Degraded) });

        let res = app
            .oneshot(Request::builder().method("POST").uri("/reboot").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_locked_out_blocks_all_including_reads() {
        let app = Router::new()
            .route("/metrics", get(ok_handler))
            .layer(AegisPolicyLayer { cache: cache_with(FleetPosture::LockedOut) });

        let res = app
            .oneshot(Request::builder().method("GET").uri("/metrics").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_missing_cache_blocks_even_read() {
        let cache: SharedPostureCache = Arc::new(RwLock::new(None));
        let app = Router::new()
            .route("/telemetry/status", get(ok_handler))
            .layer(AegisPolicyLayer { cache });

        let res = app
            .oneshot(Request::builder().method("GET").uri("/telemetry/status").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_poisoned_cache_lock_blocks_request() {
        use std::sync::Arc;
        // Poison the lock by panicking inside a write guard.
        let cache: SharedPostureCache = Arc::new(RwLock::new(None));
        let cache_clone = cache.clone();
        let _ = std::panic::catch_unwind(move || {
            let _guard = cache_clone.write().unwrap();
            panic!("intentional poison");
        });

        let app = Router::new()
            .route("/cmd_vel", post(ok_handler))
            .layer(AegisPolicyLayer { cache });

        let res = app
            .oneshot(Request::builder().method("POST").uri("/cmd_vel").body(Body::empty()).unwrap())
            .await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
