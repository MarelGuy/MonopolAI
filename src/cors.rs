use axum::http::{HeaderValue, Method, header};
use tower_http::cors::{AllowOrigin, CorsLayer};

pub fn cors_layer(origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.to_vec()))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-data-key"),
        ])
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{
            Method, Request, StatusCode,
            header::{
                ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
                ACCESS_CONTROL_REQUEST_METHOD, ORIGIN,
            },
        },
        routing::get,
    };
    use tower::ServiceExt;

    use super::cors_layer;

    const ALLOWED_ORIGIN: &str = "https://allowed.example";

    fn app() -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(cors_layer(&[ALLOWED_ORIGIN.parse().unwrap()]))
    }

    #[tokio::test]
    async fn allows_a_configured_origin() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(ORIGIN, ALLOWED_ORIGIN)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
            ALLOWED_ORIGIN
        );
    }

    #[tokio::test]
    async fn omits_the_header_for_an_unknown_origin() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(ORIGIN, "https://unknown.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none()
        );
    }

    #[tokio::test]
    async fn advertises_the_supported_methods_on_preflight() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/")
                    .header(ORIGIN, ALLOWED_ORIGIN)
                    .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let methods = response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap()
            .to_str()
            .unwrap();

        assert!(methods.contains("POST"));
        assert!(methods.contains("PUT"));
        assert!(methods.contains("DELETE"));
    }
}
