//! The user-editable text of the active kind's spec, for `{{variable}}`
//! substitution and unresolved-variable detection.

use super::ProtocolSpecs;
use crate::kind::RequestKind;

impl ProtocolSpecs {
    /// A copy with `f` applied to every text field of `kind`'s spec (other
    /// kinds' parts are left as they are).
    #[must_use]
    pub fn map_text(&self, kind: RequestKind, f: impl Fn(&str) -> String) -> Self {
        let mut out = self.clone();
        match kind {
            RequestKind::Rest => {}
            RequestKind::Graphql => {
                let g = &mut out.graphql;
                g.query = f(&g.query);
                g.variables = f(&g.variables);
                g.operation_name = f(&g.operation_name);
            }
            RequestKind::Soap => {
                let s = &mut out.soap;
                s.action = f(&s.action);
                s.envelope = f(&s.envelope);
                s.wsdl_url = f(&s.wsdl_url);
            }
            RequestKind::Odata => {
                let o = &mut out.odata;
                for v in [
                    &mut o.entity_set,
                    &mut o.filter,
                    &mut o.select,
                    &mut o.expand,
                    &mut o.orderby,
                    &mut o.top,
                    &mut o.skip,
                    &mut o.search,
                ] {
                    *v = f(v);
                }
            }
            RequestKind::Trpc => {
                let t = &mut out.trpc;
                t.procedure = f(&t.procedure);
                t.input = f(&t.input);
            }
            RequestKind::WebSocket => {
                let w = &mut out.websocket;
                w.subprotocols = f(&w.subprotocols);
                w.draft = f(&w.draft);
                for t in &mut w.templates {
                    t.payload = f(&t.payload);
                }
            }
            RequestKind::Grpc => {
                let g = &mut out.grpc;
                g.method = f(&g.method);
                g.message = f(&g.message);
                g.proto_file = f(&g.proto_file);
                g.import_paths = f(&g.import_paths);
            }
        }
        out
    }

    /// Text fields of `kind`'s spec that are sent (templates are not).
    #[must_use]
    pub fn texts(&self, kind: RequestKind) -> Vec<&str> {
        match kind {
            RequestKind::Rest => Vec::new(),
            RequestKind::Graphql => {
                let g = &self.graphql;
                vec![&g.query, &g.variables, &g.operation_name]
            }
            RequestKind::Soap => vec![&self.soap.action, &self.soap.envelope],
            RequestKind::Odata => {
                let o = &self.odata;
                vec![
                    &o.entity_set,
                    &o.filter,
                    &o.select,
                    &o.expand,
                    &o.orderby,
                    &o.top,
                    &o.skip,
                    &o.search,
                ]
            }
            RequestKind::Trpc => vec![&self.trpc.procedure, &self.trpc.input],
            RequestKind::WebSocket => vec![&self.websocket.subprotocols, &self.websocket.draft],
            RequestKind::Grpc => vec![&self.grpc.method, &self.grpc.message],
        }
        .into_iter()
        .map(String::as_str)
        .collect()
    }
}
