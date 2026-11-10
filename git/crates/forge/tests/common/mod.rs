//! Replays recorded API responses; no network. Requests are recorded so tests
//! can assert on paths, headers and bodies.

#![allow(dead_code)]

use async_trait::async_trait;
use ferrumgit_forge::{ForgeResult, HttpRequest, HttpResponse, Method, Transport};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct Fixtures {
    base: String,
    dir: &'static str,
    routes: HashMap<String, HttpResponse>,
    pub requests: Mutex<Vec<HttpRequest>>,
}

pub fn fixture(dir: &str, name: &str) -> String {
    let path = format!("{}/tests/fixtures/{dir}/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

impl Fixtures {
    pub fn new(base: &str, dir: &'static str) -> Self {
        Self {
            base: base.to_owned(),
            dir,
            routes: HashMap::new(),
            requests: Mutex::default(),
        }
    }

    /// `route` is `"GET /path?query"` relative to the API base.
    #[must_use]
    pub fn on(self, route: &str, file: &str) -> Self {
        self.on_status(route, 200, &[], file)
    }

    #[must_use]
    pub fn on_status(
        mut self,
        route: &str,
        status: u16,
        headers: &[(&str, &str)],
        file: &str,
    ) -> Self {
        let body = if file.is_empty() {
            String::new()
        } else {
            fixture(self.dir, file)
        };
        let headers = headers
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        self.routes.insert(
            route.to_owned(),
            HttpResponse {
                status,
                headers,
                body,
            },
        );
        self
    }

    pub fn sent(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("lock").clone()
    }

    pub fn sent_to(&self, route: &str) -> Option<HttpRequest> {
        self.sent().into_iter().find(|r| self.key(r) == route)
    }

    fn key(&self, r: &HttpRequest) -> String {
        let path = r.url.strip_prefix(&self.base).unwrap_or(&r.url);
        format!("{} {path}", r.method.as_str())
    }
}

#[async_trait]
impl Transport for Fixtures {
    async fn send(&self, request: HttpRequest) -> ForgeResult<HttpResponse> {
        let key = self.key(&request);
        self.requests.lock().expect("lock").push(request);
        let Some(response) = self.routes.get(&key).cloned() else {
            panic!("no fixture for {key}");
        };
        Ok(response)
    }
}

pub fn method_is(r: &HttpRequest, m: Method) -> bool {
    r.method == m
}
