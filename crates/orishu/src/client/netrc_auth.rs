use netrc::{Netrc, Result};
use reqwest_middleware::{RequestBuilder, RequestInitialiser};
use std::path::Path;

pub struct NetrcMiddleware {
    nrc: Netrc,
}

#[allow(dead_code)]
impl NetrcMiddleware {
    pub fn new() -> Result<Self> {
        Netrc::new().map(|nrc| NetrcMiddleware { nrc })
    }

    pub fn from_file(file: &Path) -> Result<Self> {
        Netrc::from_file(file).map(|nrc| NetrcMiddleware { nrc })
    }
}

impl RequestInitialiser for NetrcMiddleware {
    fn init(&self, req: RequestBuilder) -> RequestBuilder {
        match req.try_clone() {
            Some(nr) => req
                .try_clone()
                .unwrap()
                .build()
                .ok()
                .and_then(|r| {
                    r.url()
                        .host_str()
                        .and_then(|host| {
                            self.nrc
                                .hosts
                                .get(host)
                                .or_else(|| self.nrc.hosts.get("default"))
                        })
                        .map(|auth| {
                            nr.basic_auth(
                                &auth.login,
                                if auth.password.is_empty() {
                                    None
                                } else {
                                    Some(&auth.password)
                                },
                            )
                        })
                })
                .unwrap_or(req),
            None => req,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use reqwest::Client;
    use reqwest_middleware::ClientBuilder;
    use std::path::PathBuf;
    use wiremock::matchers::{basic_auth, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const NETRC: &str = r#"default login myuser password mypassword"#;

    fn create_netrc_file() -> PathBuf {
        let dest = std::env::temp_dir().join("netrc");
        if !dest.exists() {
            std::fs::write(&dest, NETRC).unwrap();
        }
        dest
    }

    #[tokio::test]
    async fn test_init() {
        let server = MockServer::start().await;

        Mock::given(method(http::Method::GET))
            .and(path("/collection"))
            .and(basic_auth("myuser", "mypassword"))
            .respond_with(ResponseTemplate::new(http::StatusCode::OK))
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

        let file = create_netrc_file();

        let status = ClientBuilder::new(Client::new())
            .with_init(NetrcMiddleware::from_file(file.as_path()).unwrap())
            .build()
            .get(format!("{}/collection", &server.uri()))
            .send()
            .await
            .unwrap()
            .status();

        assert_eq!(status, http::StatusCode::OK);
    }
}
