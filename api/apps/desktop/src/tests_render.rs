//! Headless GPUI smoke test: renders a tab of every request kind with each
//! of its sections, loaded descriptions, a WebSocket log and protocol
//! responses, plus the "New request" dialog. No window is shown.

use crate::app::FerrumApi;
use crate::state::protocols::{Loadable, WsSession, WsStatus};
use crate::state::{BodyMode, Dialog, RequestSection, TabResponse};
use ferrumapi_application::AppServices;
use ferrumapi_application::domain::protocol::graphql_schema::parse_introspection;
use ferrumapi_application::domain::protocol::grpc::{GrpcMethodInfo, GrpcStatus};
use ferrumapi_application::http::ws::WsIncoming;
use ferrumapi_application::interop::odata::{ODataEntitySet, ODataMetadata};
use ferrumapi_application::interop::wsdl::WsdlOperation;
use ferrumapi_domain::protocol::SoapVersion;
use ferrumapi_domain::{Collection, HttpRequest, HttpResponse, RequestKind, RequestTab, TabId};
use gpui::{App, Entity, Font, Pixels, TestAppContext, VisualTestContext, font, px};
use theme::{ThemeSettingsProvider, UiDensity};

struct TestFonts {
    ui: Font,
    buffer: Font,
}

impl ThemeSettingsProvider for TestFonts {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui
    }
    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.buffer
    }
    fn ui_font_size(&self, _: &App) -> Pixels {
        px(14.)
    }
    fn buffer_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }
    fn ui_density(&self, _: &App) -> UiDensity {
        UiDensity::Default
    }
}

fn app_window(
    cx: &mut TestAppContext,
) -> (
    Entity<FerrumApi>,
    &mut VisualTestContext,
    std::path::PathBuf,
) {
    cx.update(|cx| {
        theme::set_theme_settings_provider(
            Box::new(TestFonts {
                ui: font("Helvetica"),
                buffer: font("Menlo"),
            }),
            cx,
        );
        theme::init(theme::LoadThemes::JustBase, cx);
    });
    let dir = std::env::temp_dir().join(format!("ferrumapi-ui-{}", uuid::Uuid::new_v4()));
    let services = AppServices::start(&dir).expect("services");
    // The GPUI test scheduler must not be woken from the services runtime's
    // threads, so the view is built without loading local data.
    let (app, cx) = cx.add_window_view(move |window, cx| {
        let mut app = FerrumApi::without_loading(services, window, cx);
        app.state.loaded = true;
        app.new_tab();
        app
    });
    cx.run_until_parked();
    (app, cx, dir)
}

fn render(app: &Entity<FerrumApi>, cx: &mut VisualTestContext, f: impl FnOnce(&mut FerrumApi)) {
    app.update(cx, |app, cx| {
        f(app);
        cx.notify();
    });
    cx.run_until_parked();
}

fn has_field(app: &Entity<FerrumApi>, cx: &mut VisualTestContext, key: String) -> bool {
    app.read_with(cx, |a, _| a.fields.entity(&key).is_some())
}

fn response(body: &str, content_type: &str) -> TabResponse {
    TabResponse::Done(Box::new(HttpResponse {
        status: 200,
        status_text: "OK".into(),
        headers: vec![("content-type".into(), content_type.into())],
        body: body.as_bytes().to_vec(),
        size_bytes: body.len() as u64,
        ..HttpResponse::default()
    }))
}

/// Fills in what the kind's editors and response view can show.
fn decorate(app: &mut FerrumApi, id: TabId, kind: RequestKind) {
    let p = &mut app.state.protocols;
    match kind {
        RequestKind::Graphql => {
            let schema = parse_introspection(
                br#"{"data":{"__schema":{"queryType":{"name":"Query"},"types":[
                {"kind":"OBJECT","name":"Query","fields":[{"name":"me","args":[],"type":{"kind":"OBJECT","name":"User"}}]},
                {"kind":"OBJECT","name":"User","fields":[{"name":"id","args":[],"type":{"kind":"SCALAR","name":"ID"}}]}]}}}"#,
            )
            .expect("schema");
            p.graphql.insert(id, Loadable::Ready(schema));
            p.schema_open = Some("User".into());
            app.state
                .edit_request(id, |r| r.specs.graphql.query = "{ me { ".into());
        }
        RequestKind::Soap => {
            p.wsdl.insert(
                id,
                Loadable::Ready(vec![WsdlOperation {
                    name: "Add".into(),
                    action: "urn:Add".into(),
                    version: SoapVersion::V11,
                    endpoint: None,
                    sample_body: "<Add/>".into(),
                }]),
            );
            app.state
                .responses
                .insert(id, response("<a><b x=\"1\">2</b></a>", "text/xml"));
        }
        RequestKind::Odata => {
            p.odata.insert(
                id,
                Loadable::Ready(ODataMetadata {
                    version: "4.0".into(),
                    entity_sets: vec![ODataEntitySet {
                        name: "People".into(),
                        ..Default::default()
                    }],
                }),
            );
        }
        RequestKind::Trpc => {
            app.state.body_mode = BodyMode::Data;
            app.state.responses.insert(
                id,
                response(r#"{"result":{"data":{"id":1}}}"#, "application/json"),
            );
        }
        RequestKind::Grpc => {
            p.grpc.insert(
                id,
                Loadable::Ready(vec![GrpcMethodInfo {
                    full_name: "pkg.S/M".into(),
                    input_template: "{}".into(),
                    ..Default::default()
                }]),
            );
            app.state
                .edit_request(id, |r| r.specs.grpc.method = "pkg.S/M".into());
            let mut done = response("{\"n\": 1}", "application/json");
            if let TabResponse::Done(r) = &mut done {
                r.grpc = Some(GrpcStatus {
                    code: 5,
                    name: "NOT_FOUND".into(),
                    message: "missing".into(),
                });
                r.trailers = vec![("grpc-status".into(), "5".into())];
            }
            app.state.responses.insert(id, done);
        }
        RequestKind::WebSocket => {
            let mut s = WsSession::connecting(1);
            s.status = WsStatus::Open;
            s.sent(
                ferrumapi_domain::protocol::websocket::WsFrame::Text,
                "{\"a\":1}".into(),
                7,
            );
            s.receive(WsIncoming::Text("pong".into()));
            p.ws.insert(id, s);
        }
        RequestKind::Rest => {}
    }
}

#[gpui::test]
fn every_kind_and_section_renders(cx: &mut TestAppContext) {
    let (app, cx, dir) = app_window(cx);
    for kind in RequestKind::ALL {
        let tab = RequestTab::new(kind.label(), HttpRequest::of_kind(kind));
        let id = tab.id;
        render(&app, cx, |app| {
            app.push_tab(tab);
            decorate(app, id, kind);
        });
        for section in RequestSection::for_kind(kind) {
            render(&app, cx, |app| app.state.section = *section);
        }
        assert!(
            has_field(&app, cx, format!("url:{id}")),
            "{kind:?} URL field rendered"
        );
    }
    // Spot checks that kind editors really rendered.
    let ids: Vec<(RequestKind, TabId)> = app.read_with(cx, |a, _| {
        a.state
            .tabs
            .iter()
            .map(|t| (t.request.kind, t.id))
            .collect()
    });
    let id_of = |k: RequestKind| {
        ids.iter()
            .find(|(kind, _)| *kind == k)
            .map(|(_, id)| *id)
            .expect("tab")
    };
    render(&app, cx, |app| {
        app.activate(id_of(RequestKind::Graphql));
        app.state.section = RequestSection::Query;
    });
    assert!(has_field(
        &app,
        cx,
        format!("gql-query:{}", id_of(RequestKind::Graphql))
    ));
    render(&app, cx, |app| {
        app.activate(id_of(RequestKind::WebSocket));
        app.state.section = RequestSection::Message;
    });
    assert!(has_field(
        &app,
        cx,
        format!("ws-draft:{}", id_of(RequestKind::WebSocket))
    ));

    let c = Collection::new("C");
    let cid = c.id;
    render(&app, cx, |app| {
        app.state.collections.push(c);
        app.state.dialog = Some(Dialog::NewRequest {
            target: (cid, None),
        });
    });
    assert!(app.read_with(cx, |a, _| a.state.dialog.is_some()));
    let _ = std::fs::remove_dir_all(dir);
}
