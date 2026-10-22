pub mod dialogs;
pub mod request;
pub mod response;
pub mod rust_ui;
pub mod shell;
pub mod sidebar;
pub mod widgets;

use ferrumapi_domain::HttpMethod;

/// Postman-style method colour. Literal strings so the Tailwind scanner keeps them.
#[must_use]
pub const fn method_class(m: HttpMethod) -> &'static str {
    match m {
        HttpMethod::Get => "text-green-600 dark:text-green-400",
        HttpMethod::Post => "text-amber-600 dark:text-amber-400",
        HttpMethod::Put => "text-blue-600 dark:text-blue-400",
        HttpMethod::Patch => "text-purple-600 dark:text-purple-400",
        HttpMethod::Delete => "text-red-600 dark:text-red-400",
        HttpMethod::Head | HttpMethod::Options => "text-muted-foreground",
    }
}

/// Status-code colour class (2xx success, 3xx info, 4xx warning, 5xx destructive).
#[must_use]
pub const fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "text-success",
        300..=399 => "text-info",
        400..=499 => "text-warning",
        _ => "text-destructive",
    }
}
