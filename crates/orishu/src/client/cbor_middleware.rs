use async_trait::async_trait;
use http::Extensions;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};

const CBOR_CONTENT_TYPE: reqwest::header::HeaderValue =
    reqwest::header::HeaderValue::from_static("application/cbor");

#[derive(Debug, Clone, Default)]
pub struct CborContentMiddleware {}

#[async_trait]
impl Middleware for CborContentMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        req.headers_mut()
            .insert(reqwest::header::CONTENT_TYPE, CBOR_CONTENT_TYPE);
        next.run(req, extensions).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use reqwest::Client;
    use reqwest_middleware::ClientBuilder;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_init() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::POST))
            .and(path("/collection"))
            .and(header("Content-Type", "application/cbor"))
            .respond_with(ResponseTemplate::new(http::StatusCode::NO_CONTENT))
            .mount(&server)
            .await;
        Mock::given(method(http::Method::POST))
            .and(path("/collection"))
            .respond_with(ResponseTemplate::new(
                http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ))
            .mount(&server)
            .await;

        let status = ClientBuilder::new(Client::new())
            .build()
            .post(format!("{}/collection", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, http::StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let status = ClientBuilder::new(Client::new())
            .with(CborContentMiddleware::default())
            .build()
            .post(format!("{}/collection", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, http::StatusCode::NO_CONTENT);
    }
}
