use crate::error::map_mongo_error;
use ferrumdata_domain::{DbError, DbErrorKind, Endpoint, ResolvedConnectionConfig, Secret};
use mongodb::options::{ClientOptions, Credential};

/// Connections per profile; conservative to respect server limits.
const MAX_POOL_SIZE: u32 = 4;

/// Builds driver options from a profile: the connection string (which may set
/// TLS, replica set, auth source, …) plus the user/password fields. The
/// password is applied here and nowhere else.
pub async fn client_options(
    config: &ResolvedConnectionConfig,
    secret: &Secret,
) -> Result<ClientOptions, DbError> {
    let profile = &config.profile;
    let Endpoint::Uri { uri } = &profile.endpoint else {
        return Err(DbError::new(
            DbErrorKind::Connection,
            "MongoDB requires a connection string",
        ));
    };
    // `mongodb+srv://` resolves DNS here, so this can take a moment.
    let mut options =
        tokio::time::timeout(profile.connect_timeout(), ClientOptions::parse(uri.trim()))
            .await
            .map_err(|_| {
                DbError::new(
                    DbErrorKind::Timeout,
                    "Timed out resolving the connection string",
                )
            })?
            .map_err(|e| map_mongo_error(&e, secret))?;
    apply_profile(&mut options, config);
    Ok(options)
}

/// Profile settings layered over parsed options (separate for unit tests).
pub fn apply_profile(options: &mut ClientOptions, config: &ResolvedConnectionConfig) {
    let profile = &config.profile;
    options.app_name = Some("FerrumData".into());
    options.connect_timeout = Some(profile.connect_timeout());
    options.server_selection_timeout = Some(profile.connect_timeout());
    options.max_pool_size = Some(MAX_POOL_SIZE);
    let password = config
        .password
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| p.expose().to_owned());
    let username = profile
        .username
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(str::to_owned);
    match (options.credential.as_mut(), username) {
        // Keep mechanism/auth source from the connection string, if any.
        (Some(credential), Some(user)) => {
            credential.username = Some(user);
            credential.password = password;
        }
        (Some(credential), None) => {
            if password.is_some() {
                credential.password = password;
            }
        }
        (None, Some(user)) => {
            options.credential = Some(
                Credential::builder()
                    .username(user)
                    .password(password)
                    .build(),
            );
        }
        (None, None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ConnectionProfile, DatabaseEngine};

    fn config(user: Option<&str>, password: Option<&str>) -> ResolvedConnectionConfig {
        let mut profile = ConnectionProfile::new(DatabaseEngine::MongoDb, "m");
        profile.username = user.map(str::to_owned);
        ResolvedConnectionConfig {
            profile,
            password: password.map(Secret::new),
        }
    }

    #[tokio::test]
    async fn parses_uri_and_applies_credentials() {
        let cfg = config(Some("alice"), Some("pw"));
        let opts = client_options(&cfg, &Secret::new("pw"))
            .await
            .expect("options");
        let cred = opts.credential.expect("credential");
        assert_eq!(cred.username.as_deref(), Some("alice"));
        assert_eq!(cred.password.as_deref(), Some("pw"));
        assert_eq!(opts.app_name.as_deref(), Some("FerrumData"));
        assert_eq!(opts.max_pool_size, Some(MAX_POOL_SIZE));
    }

    #[tokio::test]
    async fn keeps_auth_source_from_the_connection_string() {
        let mut cfg = config(None, Some("pw"));
        cfg.profile.endpoint = Endpoint::Uri {
            uri: "mongodb://bob@localhost:27017/?authSource=admin".into(),
        };
        let opts = client_options(&cfg, &Secret::new("pw"))
            .await
            .expect("options");
        let cred = opts.credential.expect("credential");
        assert_eq!(cred.username.as_deref(), Some("bob"));
        assert_eq!(cred.source.as_deref(), Some("admin"));
        assert_eq!(cred.password.as_deref(), Some("pw"));
    }

    #[tokio::test]
    async fn no_user_means_no_credential() {
        let opts = client_options(&config(None, None), &Secret::new(""))
            .await
            .expect("options");
        assert!(opts.credential.is_none());
    }

    #[tokio::test]
    async fn rejects_non_uri_endpoints_and_bad_strings() {
        let mut cfg = config(None, None);
        cfg.profile.endpoint = Endpoint::Tcp {
            host: "h".into(),
            port: 1,
        };
        assert!(client_options(&cfg, &Secret::new("")).await.is_err());
        cfg.profile.endpoint = Endpoint::Uri {
            uri: "mongodb://".into(),
        };
        assert!(client_options(&cfg, &Secret::new("")).await.is_err());
    }
}
