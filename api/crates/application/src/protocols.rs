//! Protocol-specific services: GraphQL introspection, WSDL operations,
//! OData `$metadata`, gRPC schemas and calls, WebSocket sessions.
//!
//! Descriptions are fetched with the request's own headers, auth and
//! variables, and cached (SQLite for GraphQL / WSDL / OData, memory for gRPC
//! descriptor pools) per substituted endpoint.

use crate::{AppServices, Prepared};
use ferrumapi_domain::protocol::graphql::introspection_request;
use ferrumapi_domain::protocol::graphql_schema::{GqlSchema, parse_introspection};
use ferrumapi_domain::protocol::grpc::{GrpcMethodInfo, GrpcSchemaSource};
use ferrumapi_domain::protocol::odata::metadata_url;
use ferrumapi_domain::{
    ExecError, HistoryEntry, HttpMethod, HttpRequest, HttpResponse, KeyValue, RequestBody,
    RequestKind, RequestOrigin,
};
use ferrumapi_grpc::DescriptorPool;
use ferrumapi_http::CancellationToken;
use ferrumapi_http::substitute;
use ferrumapi_http::ws::{WsClient, WsConnected, WsIncoming};
use ferrumapi_interop::odata::{ODataMetadata, parse_metadata};
use ferrumapi_interop::wsdl::{WsdlOperation, parse as parse_wsdl};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::mpsc::UnboundedReceiver;

/// gRPC descriptor pools by schema key (endpoint or .proto path).
pub(crate) type GrpcPools = Arc<Mutex<HashMap<String, DescriptorPool>>>;

/// A live WebSocket: its handshake, a sender and the incoming frames.
pub type WsSession = (WsConnected, WsClient, UnboundedReceiver<WsIncoming>);

fn body_error(resp: &HttpResponse) -> String {
    format!(
        "the server answered HTTP {} {}",
        resp.status, resp.status_text
    )
}

/// The substituted request (cache keys, gRPC calls).
#[must_use]
pub fn concrete(prepared: &Prepared) -> HttpRequest {
    substitute(&prepared.request, &prepared.variables)
}

/// The same request as a plain GET to `url` (WSDL, `$metadata`).
fn get(prepared: &Prepared, url: String, accept: &str) -> Prepared {
    let mut request = prepared.request.clone();
    request.kind = RequestKind::Rest;
    request.method = HttpMethod::Get;
    request.url = url;
    request.body = RequestBody::None;
    request.params.clear();
    request
        .headers
        .retain(|h| !h.key.trim().eq_ignore_ascii_case("content-type"));
    if !request
        .headers
        .iter()
        .any(|h| h.is_active() && h.key.trim().eq_ignore_ascii_case("accept"))
    {
        request.headers.push(KeyValue::new("Accept", accept));
    }
    Prepared {
        request,
        variables: prepared.variables.clone(),
    }
}

fn grpc_key(request: &HttpRequest) -> String {
    let spec = &request.specs.grpc;
    match spec.source {
        GrpcSchemaSource::Reflection => format!("reflect:{}", request.url.trim()),
        GrpcSchemaSource::ProtoFile => {
            format!(
                "proto:{}|{}",
                spec.proto_file.trim(),
                spec.import_paths.trim()
            )
        }
    }
}

impl AppServices {
    async fn fetch(&self, prepared: Prepared) -> Result<HttpResponse, String> {
        self.execute(prepared, CancellationToken::new())
            .await
            .map_err(|e| e.to_string())
    }

    /// The endpoint's schema: cached unless `refresh`, else introspected.
    pub async fn graphql_schema(
        &self,
        prepared: Prepared,
        refresh: bool,
    ) -> Result<GqlSchema, String> {
        let endpoint = concrete(&prepared).url.trim().to_owned();
        if !refresh && let Ok(Some(schema)) = self.store.load_schema("graphql", &endpoint).await {
            return Ok(schema);
        }
        let request = introspection_request(&prepared.request);
        let resp = self
            .fetch(Prepared {
                request,
                variables: prepared.variables,
            })
            .await?;
        let schema = parse_introspection(&resp.body).map_err(|e| {
            if (200..300).contains(&resp.status) {
                e
            } else {
                body_error(&resp)
            }
        })?;
        if let Err(e) = self.store.save_schema("graphql", &endpoint, &schema).await {
            tracing::warn!(error = %e, "could not cache the GraphQL schema");
        }
        Ok(schema)
    }

    /// Operations of the WSDL at `request.specs.soap.wsdl_url`.
    pub async fn wsdl_operations(
        &self,
        prepared: Prepared,
        refresh: bool,
    ) -> Result<Vec<WsdlOperation>, String> {
        let url = prepared
            .variables
            .resolve(prepared.request.specs.soap.wsdl_url.trim());
        if url.is_empty() {
            return Err("enter the WSDL URL (often the endpoint followed by ?wsdl)".into());
        }
        if !refresh && let Ok(Some(ops)) = self.store.load_schema("wsdl", &url).await {
            return Ok(ops);
        }
        let resp = self
            .fetch(get(&prepared, url.clone(), "text/xml, application/xml"))
            .await?;
        if !(200..300).contains(&resp.status) {
            return Err(body_error(&resp));
        }
        let ops = parse_wsdl(&resp.body_text()).map_err(|e| e.to_string())?;
        if let Err(e) = self.store.save_schema("wsdl", &url, &ops).await {
            tracing::warn!(error = %e, "could not cache the WSDL");
        }
        Ok(ops)
    }

    /// Entity sets from `<service root>/$metadata`.
    pub async fn odata_metadata(
        &self,
        prepared: Prepared,
        refresh: bool,
    ) -> Result<ODataMetadata, String> {
        let url = metadata_url(&concrete(&prepared).url);
        if !refresh && let Ok(Some(m)) = self.store.load_schema("odata", &url).await {
            return Ok(m);
        }
        let mut get = get(&prepared, url.clone(), "application/xml");
        // The authored URL may hold variables; send the substituted one.
        get.request.url.clone_from(&url);
        let resp = self.fetch(get).await?;
        if !(200..300).contains(&resp.status) {
            return Err(body_error(&resp));
        }
        let metadata = parse_metadata(&resp.body_text()).map_err(|e| e.to_string())?;
        if let Err(e) = self.store.save_schema("odata", &url, &metadata).await {
            tracing::warn!(error = %e, "could not cache OData metadata");
        }
        Ok(metadata)
    }

    async fn grpc_pool(
        &self,
        request: &HttpRequest,
        refresh: bool,
    ) -> Result<DescriptorPool, ExecError> {
        let key = grpc_key(request);
        if !refresh
            && let Some(pool) = self
                .grpc_pools
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
        {
            return Ok(pool.clone());
        }
        let pool = ferrumapi_grpc::load_schema(request).await?;
        self.grpc_pools
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, pool.clone());
        Ok(pool)
    }

    /// Services and methods via reflection or the .proto file.
    pub async fn grpc_methods(
        &self,
        prepared: Prepared,
        refresh: bool,
    ) -> Result<Vec<GrpcMethodInfo>, String> {
        let request = concrete(&prepared);
        let pool = self
            .grpc_pool(&request, refresh)
            .await
            .map_err(|e| e.to_string())?;
        Ok(ferrumapi_grpc::methods(&pool))
    }

    /// One gRPC call (schema loaded on first use).
    pub(crate) async fn grpc_call(
        &self,
        prepared: Prepared,
        cancel: CancellationToken,
    ) -> Result<HttpResponse, ExecError> {
        let request = concrete(&prepared);
        let pool = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(ExecError::Cancelled),
            p = self.grpc_pool(&request, false) => p?,
        };
        ferrumapi_grpc::invoke(&request, &pool, cancel).await
    }

    /// Opens a WebSocket and records the connection in history (status 101).
    pub async fn ws_connect(
        &self,
        authored: HttpRequest,
        prepared: Prepared,
        origin: Option<RequestOrigin>,
    ) -> Result<WsSession, ExecError> {
        let result = ferrumapi_http::ws::connect(&prepared.request, &prepared.variables).await;
        let mut entry = HistoryEntry::new(authored, origin);
        match &result {
            Ok((connected, _, _)) => entry.status = Some(connected.status),
            Err(e) => entry.error = Some(e.to_string()),
        }
        let limit = self.history_limit().await;
        if let Err(e) = self.store.add_history(&entry, limit).await {
            tracing::warn!(error = %e, "failed to record history");
        }
        result
    }
}
