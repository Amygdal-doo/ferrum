//! Response headers and cookies tables.

use dioxus::prelude::*;
use ferrumapi_domain::ResponseCookie;

const TH: &str = "sticky top-0 border-b bg-background px-3 py-1 text-left text-[11px] font-medium text-muted-foreground";
const TD: &str = "border-b px-3 py-1 align-top font-mono text-xs select-text break-all";

#[component]
pub fn HeadersTable(headers: Vec<(String, String)>) -> Element {
    if headers.is_empty() {
        return rsx! { p { class: "p-3 text-xs text-muted-foreground", "No headers." } };
    }
    rsx! {
        div { class: "h-full overflow-auto",
            table { class: "w-full border-collapse",
                thead { tr { th { class: "{TH} w-1/3", "Name" } th { class: TH, "Value" } } }
                tbody {
                    for (i, (k, v)) in headers.into_iter().enumerate() {
                        tr { key: "{i}", td { class: "{TD} font-semibold", "{k}" } td { class: TD, "{v}" } }
                    }
                }
            }
        }
    }
}

#[component]
pub fn CookiesTable(cookies: Vec<ResponseCookie>) -> Element {
    if cookies.is_empty() {
        return rsx! { p { class: "p-3 text-xs text-muted-foreground", "No cookies were set by this response." } };
    }
    let yes = |b: bool| if b { "Yes" } else { "" };
    rsx! {
        div { class: "h-full overflow-auto",
            table { class: "w-full border-collapse",
                thead {
                    tr {
                        for h in ["Name", "Value", "Domain", "Path", "Expires", "HttpOnly", "Secure"] {
                            th { key: "{h}", class: TH, "{h}" }
                        }
                    }
                }
                tbody {
                    for (i, c) in cookies.into_iter().enumerate() {
                        tr { key: "{i}",
                            td { class: "{TD} font-semibold", "{c.name}" }
                            td { class: TD, "{c.value}" }
                            td { class: TD, "{c.domain.clone().unwrap_or_default()}" }
                            td { class: TD, "{c.path.clone().unwrap_or_default()}" }
                            td { class: TD, "{c.expires.clone().unwrap_or_default()}" }
                            td { class: TD, "{yes(c.http_only)}" }
                            td { class: TD, "{yes(c.secure)}" }
                        }
                    }
                }
            }
        }
    }
}
