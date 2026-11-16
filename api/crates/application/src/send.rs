//! Sending a request: resolve inherited auth and variables, execute, record history.

use crate::{AppServices, DEFAULT_HISTORY_LIMIT};
use ferrumapi_domain::{
    Auth, Collection, Environment, EnvironmentId, ExecError, HistoryEntry, HttpRequest,
    HttpResponse, RequestKind, RequestOrigin, VariableSet,
};
use ferrumapi_http::CancellationToken;
use ferrumapi_interop::codegen::{self, CodeTarget};

/// A request ready to execute: auth resolved (never `Inherit`), variables
/// collected but not yet substituted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub request: HttpRequest,
    pub variables: VariableSet,
    /// The active environment (OAuth 2 tokens are cached per environment).
    pub environment: Option<EnvironmentId>,
}

fn find_collection(
    collections: &[Collection],
    origin: Option<RequestOrigin>,
) -> Option<&Collection> {
    let o = origin?;
    collections.iter().find(|c| c.id == o.collection)
}

/// Variables visible to a request: globals < collection < active environment.
#[must_use]
pub fn variables_for(
    origin: Option<RequestOrigin>,
    collections: &[Collection],
    globals: &Environment,
    environment: Option<&Environment>,
) -> VariableSet {
    let coll: &[_] = find_collection(collections, origin).map_or(&[], |c| &c.variables);
    let env: &[_] = environment.map_or(&[], |e| &e.variables);
    VariableSet::from_layers(&[&globals.variables, coll, env])
}

/// Resolves `Auth::Inherit` against the request's collection using the
/// tab's current (possibly unsaved) auth, and gathers variables.
#[must_use]
pub fn prepare(
    request: &HttpRequest,
    origin: Option<RequestOrigin>,
    collections: &[Collection],
    globals: &Environment,
    environment: Option<&Environment>,
) -> Prepared {
    let mut req = request.clone();
    if req.auth.is_inherit() {
        req.auth = match (find_collection(collections, origin), origin) {
            (Some(c), Some(o)) => {
                let mut patched = c.clone();
                if let Some(saved) = patched.find_request_mut(o.item) {
                    saved.request.auth = Auth::Inherit;
                }
                patched.effective_auth(o.item)
            }
            _ => Auth::None,
        };
    }
    Prepared {
        request: req,
        variables: variables_for(origin, collections, globals, environment),
        environment: environment.map(|e| e.id),
    }
}

/// Code snippet with variables substituted and auth resolved.
#[must_use]
pub fn code_snippet(target: CodeTarget, prepared: &Prepared) -> String {
    let concrete = ferrumapi_http::substitute(&prepared.request, &prepared.variables);
    codegen::generate(target, &concrete)
}

impl AppServices {
    /// Runs the pre-request script, executes `prepared`, runs the test
    /// script (its report is on the response) and records `authored`
    /// (pre-substitution) in history. Cancelled requests are not recorded.
    /// Runs on any executor but should be spawned on the services runtime.
    pub async fn send(
        &self,
        authored: HttpRequest,
        prepared: Prepared,
        origin: Option<RequestOrigin>,
        cancel: CancellationToken,
    ) -> Result<HttpResponse, ExecError> {
        let (prepared, pre) = crate::scripts::pre_request(prepared);
        let tests = prepared.request.scripts.tests.clone();
        let mut vars = prepared.variables.clone();
        let mut result = self.execute(prepared, cancel).await;
        if let Ok(response) = &mut result {
            crate::scripts::finish(response, pre, &tests, &mut vars);
        }
        if matches!(result, Err(ExecError::Cancelled)) {
            return result;
        }
        let mut entry = HistoryEntry::new(authored, origin);
        match &result {
            Ok(r) => {
                if let Some(g) = r.grpc.as_ref().filter(|g| g.code != 0) {
                    entry.error = Some(format!("gRPC {}: {}", g.name, g.message));
                }
                entry.status = Some(r.status);
                entry.time_ms = Some(r.time_ms);
                entry.size_bytes = Some(r.size_bytes);
            }
            Err(e) => entry.error = Some(e.to_string()),
        }
        let limit = self.history_limit().await;
        if let Err(e) = self.store.add_history(&entry, limit).await {
            tracing::warn!(error = %e, "failed to record history");
        }
        result
    }

    /// Executes `prepared` with cookies from the jar, then stores the
    /// response's cookies. Nothing is recorded in history.
    pub async fn execute(
        &self,
        prepared: Prepared,
        cancel: CancellationToken,
    ) -> Result<HttpResponse, ExecError> {
        let prepared = self.resolve_auth(prepared, &cancel).await?;
        match prepared.request.kind {
            RequestKind::Grpc => return self.grpc_call(prepared, cancel).await,
            RequestKind::WebSocket => {
                return Err(ExecError::InvalidRequest(
                    "WebSocket requests are opened with Connect, not sent".into(),
                ));
            }
            _ => {}
        }
        let Prepared {
            mut request,
            variables,
            ..
        } = prepared;
        self.apply_cookies(&mut request, &variables).await;
        let result = self.http.execute(&request, &variables, cancel).await;
        if let Ok(response) = &result {
            self.remember_cookies(response).await;
        }
        result
    }

    pub(crate) async fn history_limit(&self) -> usize {
        match self.store.load_settings().await {
            Ok(s) if s.history_limit > 0 => s.history_limit as usize,
            _ => DEFAULT_HISTORY_LIMIT,
        }
    }
}

#[cfg(test)]
#[path = "send_tests.rs"]
mod tests;
