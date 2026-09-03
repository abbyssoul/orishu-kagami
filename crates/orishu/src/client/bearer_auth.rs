use async_trait::async_trait;
use http::Extensions;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};

pub struct BearerMiddleware {
    token: String,
}

impl BearerMiddleware {
    pub fn with_token(token: &str) -> Self {
        Self {
            token: token.to_string(),
        }
    }
}

#[async_trait]
impl Middleware for BearerMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        req.headers_mut().insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(reqwest_middleware::Error::middleware)?,
        );
        next.run(req, extensions).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use reqwest::Client;
    use reqwest_middleware::ClientBuilder;
    use wiremock::matchers::{bearer_token, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_init() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/collection"))
            .and(bearer_token("hunter2"))
            .respond_with(ResponseTemplate::new(http::StatusCode::NO_CONTENT))
            .mount(&server)
            .await;
        Mock::given(method(http::Method::GET))
            .and(path("/collection"))
            .respond_with(ResponseTemplate::new(http::StatusCode::UNAUTHORIZED))
            .mount(&server)
            .await;

        let status = ClientBuilder::new(Client::new())
            .build()
            .get(format!("{}/collection", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, http::StatusCode::UNAUTHORIZED);

        let status = ClientBuilder::new(Client::new())
            .with(BearerMiddleware::with_token("hunter2"))
            .build()
            .get(format!("{}/collection", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, http::StatusCode::NO_CONTENT);
    }
}
