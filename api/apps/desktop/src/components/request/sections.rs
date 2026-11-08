//! Params | Auth | Headers | Body | Settings sub-tabs.

use super::body::BodyEditor;
use super::settings::SettingsEditor;
use crate::components::widgets::{AuthEditor, KvTable};
use crate::state::kv::{self, RowEdit, active_count};
use crate::state::{AppState, RequestSection};
use dioxus::prelude::*;
use ferrumapi_domain::urlparams::url_with_params;
use ferrumapi_domain::{Auth, RequestBody, RequestTab, VariableSet};

fn body_count(body: &RequestBody) -> usize {
    match body {
        RequestBody::None => 0,
        RequestBody::Raw { text, .. } => usize::from(!text.is_empty()),
        RequestBody::UrlEncoded { fields } => active_count(fields),
        RequestBody::FormData { fields } => active_count(fields),
        RequestBody::Binary { path } => usize::from(path.is_some()),
    }
}

#[component]
pub fn SectionTabs(tab: RequestTab) -> Element {
    let mut state = use_context::<AppState>();
    let current = *state.section.read();
    let r = &tab.request;
    let auth_on = usize::from(!matches!(r.auth, Auth::None | Auth::Inherit));
    let counts = [
        active_count(&r.params),
        auth_on,
        active_count(&r.headers),
        body_count(&r.body),
        0,
    ];
    rsx! {
        div { class: "flex h-9 shrink-0 items-end gap-4 border-b px-3", role: "tablist",
            for (sec, n) in RequestSection::ALL.into_iter().zip(counts) {
                {
                    let selected = sec == current;
                    let cls = if selected { "border-foreground text-foreground" } else { "border-transparent text-muted-foreground hover:text-foreground" };
                    rsx! {
                        button {
                            key: "{sec.label()}",
                            class: "flex h-8 items-center gap-1.5 border-b-2 text-xs font-medium {cls}",
                            role: "tab",
                            "aria-selected": selected,
                            onclick: move |_| state.section.set(sec),
                            "{sec.label()}"
                            if n > 0 {
                                span { class: "rounded bg-muted px-1 text-[10px] text-muted-foreground", "{n}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SectionBody(tab: RequestTab, vars: VariableSet, inherited: Option<String>) -> Element {
    let mut state = use_context::<AppState>();
    let id = tab.id;
    let revs = *state.revs.read();
    let section = *state.section.read();
    let r = tab.request.clone();

    let on_params = move |(i, edit): (usize, RowEdit)| {
        state.edit_request(id, |r| {
            kv::apply(&mut r.params, i, edit);
            r.url = url_with_params(&r.url, &r.params);
        });
        state.revs.write().url += 1;
    };
    let on_headers = move |(i, edit): (usize, RowEdit)| {
        state.edit_request(id, |r| kv::apply(&mut r.headers, i, edit));
    };

    rsx! {
        div { class: "min-h-0 flex-1 overflow-auto p-3",
            match section {
                RequestSection::Params => rsx! {
                    KvTable { key: "{id}", rows: r.params, rev: revs.params, vars, on_edit: on_params, key_placeholder: "Query parameter" }
                },
                RequestSection::Headers => rsx! {
                    KvTable { key: "{id}", rows: r.headers, rev: 0, vars, on_edit: on_headers, key_placeholder: "Header" }
                },
                RequestSection::Auth => rsx! {
                    AuthEditor {
                        auth: r.auth,
                        scope: "{id}",
                        allow_inherit: true,
                        inherited,
                        on_change: move |a: Auth| state.edit_request(id, |r| r.auth = a),
                    }
                },
                RequestSection::Body => rsx! { BodyEditor { key: "{id}", tab_id: id, body: r.body, vars, rev: revs.body } },
                RequestSection::Settings => rsx! { SettingsEditor { key: "{id}", tab_id: id, settings: r.settings } },
            }
        }
    }
}
