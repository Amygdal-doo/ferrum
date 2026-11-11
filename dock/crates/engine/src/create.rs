//! "Run container": the dialog's text fields parsed into a [`CreateSpec`]
//! (like `docker run -d -p … -e … -v … --restart … image cmd`), and that spec
//! turned into the Engine API's create body.

use bollard::models as m;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestartPolicy {
    #[default]
    No,
    Always,
    UnlessStopped,
    OnFailure,
}

impl RestartPolicy {
    pub const ALL: [Self; 4] = [Self::No, Self::Always, Self::UnlessStopped, Self::OnFailure];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::Always => "always",
            Self::UnlessStopped => "unless-stopped",
            Self::OnFailure => "on-failure",
        }
    }
}

/// `-p [ip:][host:]container[/proto]`; no host port = a random one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSpec {
    pub host_ip: Option<String>,
    pub host_port: Option<u16>,
    pub container_port: u16,
    pub protocol: String,
}

impl PortSpec {
    /// `80/tcp`, the key the API uses.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}/{}", self.container_port, self.protocol)
    }
}

/// Everything needed to create (and then start) a container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateSpec {
    pub image: String,
    pub name: Option<String>,
    /// Overrides the image's CMD when not empty.
    pub command: Vec<String>,
    /// `KEY=VALUE`.
    pub env: Vec<String>,
    pub ports: Vec<PortSpec>,
    /// Binds, `source:/target[:ro]`; a source without `/` is a named volume.
    pub volumes: Vec<String>,
    pub restart: RestartPolicy,
}

/// The run dialog's fields, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunForm {
    pub image: String,
    pub name: String,
    /// Comma- or newline-separated `-p` values.
    pub ports: String,
    /// One `KEY=VALUE` per line (`#` comments and blank lines ignored).
    pub env: String,
    /// Comma- or newline-separated `-v` values.
    pub volumes: String,
    pub command: String,
    pub restart: RestartPolicy,
}

impl RunForm {
    /// Validates every field; the error names the field and the bad value.
    /// `~/` in volume sources expands to `home`.
    pub fn parse(&self, home: Option<&Path>) -> Result<CreateSpec, String> {
        let image = self.image.trim();
        if crate::pull::split_reference(image).is_none() {
            return Err(if image.is_empty() {
                "Image: required".to_owned()
            } else {
                format!("Image: {image:?} is not a valid reference")
            });
        }
        let name = self.name.trim();
        if !name.is_empty() && !valid_name(name) {
            return Err(format!(
                "Name: {name:?} may only use letters, digits, '_', '.' and '-', and must start with a letter or digit"
            ));
        }
        let ports = items(&self.ports)
            .map(|p| {
                parse_port(p).ok_or_else(|| {
                    format!("Ports: {p:?} is not [ip:][host:]container[/tcp|udp|sctp]")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let env = self
            .env
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.split_once('=') {
                Some((k, _)) if !k.trim().is_empty() && !k.contains(char::is_whitespace) => {
                    Ok(l.to_owned())
                }
                _ => Err(format!("Environment: {l:?} is not KEY=VALUE")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let volumes = items(&self.volumes)
            .map(|v| {
                parse_volume(v, home)
                    .ok_or_else(|| format!("Volumes: {v:?} is not source:/container/path[:ro|rw]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let command = split_command(&self.command).ok_or("Command: unbalanced quotes")?;
        Ok(CreateSpec {
            image: image.to_owned(),
            name: (!name.is_empty()).then(|| name.to_owned()),
            command,
            env,
            ports,
            volumes,
            restart: self.restart,
        })
    }
}

fn items(text: &str) -> impl Iterator<Item = &str> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn valid_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

/// `8080:80`, `127.0.0.1:5432:5432/tcp`, `53/udp`, `127.0.0.1::80`.
#[must_use]
pub fn parse_port(text: &str) -> Option<PortSpec> {
    let (rest, protocol) = match text.rsplit_once('/') {
        Some((rest, proto)) if matches!(proto, "tcp" | "udp" | "sctp") => (rest, proto),
        Some(_) => return None,
        None => (text, "tcp"),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let port = |s: &str| s.parse::<u16>().ok().filter(|p| *p > 0);
    let (host_ip, host_port, container) = match parts.as_slice() {
        [c] => (None, None, *c),
        [h, c] => (None, Some(port(h)?), *c),
        [ip, h, c] if !ip.is_empty() => {
            let host = if h.is_empty() { None } else { Some(port(h)?) };
            (Some((*ip).to_owned()), host, *c)
        }
        _ => return None,
    };
    Some(PortSpec {
        host_ip,
        host_port,
        container_port: port(container)?,
        protocol: protocol.to_owned(),
    })
}

/// `./data:/data`, `~/x:/x:ro`, `pgdata:/var/lib/postgresql/data`.
#[must_use]
pub fn parse_volume(text: &str, home: Option<&Path>) -> Option<String> {
    let mut parts = text.splitn(3, ':');
    let (source, target, mode) = (parts.next()?, parts.next()?, parts.next());
    if source.is_empty() || !target.starts_with('/') {
        return None;
    }
    if mode.is_some_and(|m| !matches!(m, "ro" | "rw")) {
        return None;
    }
    let source = match (source.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest).display().to_string(),
        (Some(_), None) => return None,
        (None, _) => source.to_owned(),
    };
    Some(match mode {
        Some(mode) => format!("{source}:{target}:{mode}"),
        None => format!("{source}:{target}"),
    })
}

/// Splits like a POSIX shell (quotes and backslashes, no expansion).
/// `None` when a quote is left open.
#[must_use]
pub fn split_command(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        other => current.push(other),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => current.push(chars.next()?),
                        other => current.push(other),
                    }
                }
            }
            '\\' => {
                in_word = true;
                current.push(chars.next()?);
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            other => {
                in_word = true;
                current.push(other);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    Some(words)
}

/// The `POST /containers/create` body for `spec`.
#[must_use]
pub(crate) fn create_body(spec: &CreateSpec) -> m::ContainerCreateBody {
    let mut bindings: m::PortMap = HashMap::new();
    for p in &spec.ports {
        bindings
            .entry(p.key())
            .or_insert_with(|| Some(Vec::new()))
            .get_or_insert_with(Vec::new)
            .push(m::PortBinding {
                host_ip: p.host_ip.clone(),
                host_port: Some(p.host_port.map(|h| h.to_string()).unwrap_or_default()),
            });
    }
    let mut exposed: Vec<String> = bindings.keys().cloned().collect();
    exposed.sort();
    let restart = match spec.restart {
        RestartPolicy::No => m::RestartPolicyNameEnum::NO,
        RestartPolicy::Always => m::RestartPolicyNameEnum::ALWAYS,
        RestartPolicy::UnlessStopped => m::RestartPolicyNameEnum::UNLESS_STOPPED,
        RestartPolicy::OnFailure => m::RestartPolicyNameEnum::ON_FAILURE,
    };
    m::ContainerCreateBody {
        image: Some(spec.image.clone()),
        cmd: (!spec.command.is_empty()).then(|| spec.command.clone()),
        env: (!spec.env.is_empty()).then(|| spec.env.clone()),
        exposed_ports: (!exposed.is_empty()).then_some(exposed),
        host_config: Some(m::HostConfig {
            port_bindings: (!bindings.is_empty()).then_some(bindings),
            binds: (!spec.volumes.is_empty()).then(|| spec.volumes.clone()),
            restart_policy: Some(m::RestartPolicy {
                name: Some(restart),
                maximum_retry_count: None,
            }),
            ..m::HostConfig::default()
        }),
        ..m::ContainerCreateBody::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> RunForm {
        RunForm {
            image: "nginx".into(),
            name: "web".into(),
            ports: "8080:80, 127.0.0.1:8443:443/tcp\n53/udp".into(),
            env: "# comment\nA=1\n\nB=x=y\n".into(),
            volumes: "~/site:/usr/share/nginx/html:ro, cache:/cache".into(),
            command: r#"nginx -g "daemon off;""#.into(),
            restart: RestartPolicy::UnlessStopped,
        }
    }

    #[test]
    fn parses_the_run_form() {
        let spec = form()
            .parse(Some(Path::new("/Users/me")))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(spec.name.as_deref(), Some("web"));
        assert_eq!(spec.env, ["A=1", "B=x=y"]);
        assert_eq!(
            spec.volumes,
            ["/Users/me/site:/usr/share/nginx/html:ro", "cache:/cache"]
        );
        assert_eq!(spec.command, ["nginx", "-g", "daemon off;"]);
        assert_eq!(spec.ports.len(), 3);
        assert_eq!(spec.ports[1].host_ip.as_deref(), Some("127.0.0.1"));
        assert_eq!(spec.ports[2].key(), "53/udp");
        assert_eq!(spec.ports[2].host_port, None);
    }

    #[test]
    fn reports_the_bad_field() {
        let bad = |f: fn(&mut RunForm)| {
            let mut form = form();
            f(&mut form);
            form.parse(Some(Path::new("/h"))).err().unwrap_or_default()
        };
        assert_eq!(bad(|f| f.image.clear()), "Image: required");
        assert!(bad(|f| f.name = "-x".into()).starts_with("Name:"));
        assert!(bad(|f| f.ports = "80:99999".into()).starts_with("Ports:"));
        assert!(bad(|f| f.ports = "80/http".into()).starts_with("Ports:"));
        assert!(bad(|f| f.env = "NOVALUE".into()).starts_with("Environment:"));
        assert!(bad(|f| f.volumes = "/a:relative".into()).starts_with("Volumes:"));
        let no_home = form().parse(None).err().unwrap_or_default();
        assert!(no_home.starts_with("Volumes:"), "~ needs a home folder");
        assert_eq!(
            bad(|f| f.command = "echo 'open".into()),
            "Command: unbalanced quotes"
        );
    }

    #[test]
    fn splits_commands_like_a_shell() {
        let split = |s: &str| split_command(s).unwrap_or_default();
        assert_eq!(split("  "), Vec::<String>::new());
        assert_eq!(
            split(r#"sh -c 'echo "hi there"'"#),
            ["sh", "-c", "echo \"hi there\""]
        );
        assert_eq!(split(r"a\ b ''"), ["a b", ""]);
        assert_eq!(split(r#""a\"b""#), ["a\"b"]);
    }

    #[test]
    fn builds_the_api_body() {
        let spec = form()
            .parse(Some(Path::new("/h")))
            .unwrap_or_else(|e| panic!("{e}"));
        let body = serde_json::to_value(create_body(&spec)).unwrap_or_default();
        assert_eq!(body["Image"], "nginx");
        assert_eq!(body["ExposedPorts"]["80/tcp"], serde_json::json!({}));
        let bindings = &body["HostConfig"]["PortBindings"];
        assert_eq!(bindings["80/tcp"][0]["HostPort"], "8080");
        assert_eq!(bindings["443/tcp"][0]["HostIp"], "127.0.0.1");
        assert_eq!(bindings["53/udp"][0]["HostPort"], "");
        assert_eq!(
            body["HostConfig"]["RestartPolicy"]["Name"],
            "unless-stopped"
        );
        assert_eq!(body["Env"][1], "B=x=y");
        assert_eq!(body["Cmd"][2], "daemon off;");

        let bare = create_body(&CreateSpec {
            image: "alpine".into(),
            ..CreateSpec::default()
        });
        let bare = serde_json::to_value(bare).unwrap_or_default();
        assert!(bare.get("Cmd").is_none() && bare.get("ExposedPorts").is_none());
    }
}
