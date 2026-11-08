//! `curl ...` command line → [`HttpRequest`].

pub(crate) mod body;
mod build;
mod options;
mod tokenize;

use crate::InteropError;
use ferrumapi_domain::HttpRequest;

pub use tokenize::tokenize;

/// Parses a `curl ...` command line (bash quoting incl. `$'...'`, `\` and
/// cmd.exe `^` line continuations). Unknown options are ignored.
///
/// Defaults mirror curl: no redirects unless `-L`, POST when data or a form
/// is given, and auth [`Auth::None`](ferrumapi_domain::Auth::None) unless
/// `-u`, `--oauth2-bearer` or an `Authorization: Bearer|Basic` header is given.
pub fn parse(command: &str) -> Result<HttpRequest, InteropError> {
    let words = tokenize(command.trim())?;
    let program = words
        .first()
        .ok_or_else(|| InteropError::Parse("empty command".into()))?;
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    if !(name.eq_ignore_ascii_case("curl") || name.eq_ignore_ascii_case("curl.exe")) {
        return Err(InteropError::Parse(
            "the command must start with `curl`".into(),
        ));
    }
    build::request(options::scan(&words)?)
}
