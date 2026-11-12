//! Fetching API descriptions for the protocol editors: GraphQL schemas,
//! WSDL operations, OData `$metadata` and gRPC methods.

use crate::app::FerrumApi;
use crate::state::protocols::Loadable;
use ferrumapi_application::Prepared;
use ferrumapi_application::interop::wsdl::WsdlOperation;
use ferrumapi_domain::TabId;
use ferrumapi_domain::protocol::soap::envelope;
use gpui::Context;

impl FerrumApi {
    fn prepared_for(&self, tab_id: TabId) -> Option<Prepared> {
        let tab = self.state.tab(tab_id)?;
        Some(self.state.prepare_tab(tab))
    }

    pub fn load_graphql_schema(&mut self, tab_id: TabId, refresh: bool, cx: &mut Context<Self>) {
        let Some(prepared) = self.prepared_for(tab_id) else {
            return;
        };
        self.state
            .protocols
            .graphql
            .insert(tab_id, Loadable::Loading);
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.graphql_schema(prepared, refresh).await },
            move |this, result, _| {
                let state = match result {
                    Ok(schema) => Loadable::Ready(schema),
                    Err(e) => Loadable::Failed(e),
                };
                this.state.protocols.graphql.insert(tab_id, state);
            },
        );
    }

    pub fn load_wsdl(&mut self, tab_id: TabId, refresh: bool, cx: &mut Context<Self>) {
        let Some(prepared) = self.prepared_for(tab_id) else {
            return;
        };
        self.state.protocols.wsdl.insert(tab_id, Loadable::Loading);
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.wsdl_operations(prepared, refresh).await },
            move |this, result, _| {
                let state = match result {
                    Ok(ops) => Loadable::Ready(ops),
                    Err(e) => Loadable::Failed(e),
                };
                this.state.protocols.wsdl.insert(tab_id, state);
            },
        );
    }

    /// Fills action, version and a sample envelope from a WSDL operation
    /// (and the URL from its address when the URL is empty).
    pub fn apply_wsdl_operation(&mut self, tab_id: TabId, op: &WsdlOperation) {
        let op = op.clone();
        self.state.edit_request(tab_id, move |r| {
            let soap = &mut r.specs.soap;
            soap.operation.clone_from(&op.name);
            soap.action.clone_from(&op.action);
            soap.version = op.version;
            soap.envelope = envelope(op.version, &op.sample_body);
            if r.url.trim().is_empty()
                && let Some(endpoint) = op.endpoint
            {
                r.url = endpoint;
            }
        });
    }

    pub fn load_odata_metadata(&mut self, tab_id: TabId, refresh: bool, cx: &mut Context<Self>) {
        let Some(prepared) = self.prepared_for(tab_id) else {
            return;
        };
        self.state.protocols.odata.insert(tab_id, Loadable::Loading);
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.odata_metadata(prepared, refresh).await },
            move |this, result, _| {
                let state = match result {
                    Ok(m) => Loadable::Ready(m),
                    Err(e) => Loadable::Failed(e),
                };
                this.state.protocols.odata.insert(tab_id, state);
            },
        );
    }

    pub fn load_grpc_methods(&mut self, tab_id: TabId, refresh: bool, cx: &mut Context<Self>) {
        let Some(prepared) = self.prepared_for(tab_id) else {
            return;
        };
        self.state.protocols.grpc.insert(tab_id, Loadable::Loading);
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.grpc_methods(prepared, refresh).await },
            move |this, result, _| {
                let state = match result {
                    Ok(methods) => Loadable::Ready(methods),
                    Err(e) => Loadable::Failed(e),
                };
                this.state.protocols.grpc.insert(tab_id, state);
            },
        );
    }
}
