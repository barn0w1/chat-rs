use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    reqwest,
};
use tokio::sync::{Mutex, RwLock};
use url::Url;

use crate::config::OidcConfig;

use super::{AuthError, ConsumedOidcLogin, OidcLoginTransaction, SecretToken, VerifiedIdentity};

const CALLBACK_PATH: &str = "auth/oidc/callback";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_FORCED_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

type ConfiguredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Clone)]
pub(crate) struct OidcProvider {
    inner: Arc<ProviderInner>,
}

struct ProviderInner {
    config: OidcConfig,
    redirect_url: RedirectUrl,
    http: reqwest::Client,
    client: RwLock<ConfiguredClient>,
    refresh: Mutex<RefreshState>,
}

struct RefreshState {
    generation: u64,
    last_refresh: Option<Instant>,
}

impl fmt::Debug for OidcProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcProvider")
            .field("issuer", &self.inner.config.issuer())
            .finish_non_exhaustive()
    }
}

impl OidcProvider {
    pub(crate) async fn discover(config: OidcConfig, public_url: &Url) -> Result<Self, OidcError> {
        let callback = public_url
            .join(CALLBACK_PATH)
            .map_err(|error| OidcError::Configuration(error.to_string()))?;
        let redirect_url = RedirectUrl::new(callback.to_string())
            .map_err(|error| OidcError::Configuration(error.to_string()))?;
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| OidcError::Http(error.to_string()))?;
        let client = discover_client(&config, &redirect_url, &http).await?;

        Ok(Self {
            inner: Arc::new(ProviderInner {
                config,
                redirect_url,
                http,
                client: RwLock::new(client),
                refresh: Mutex::new(RefreshState {
                    generation: 0,
                    last_refresh: None,
                }),
            }),
        })
    }

    pub(crate) async fn begin_login(&self) -> Result<(Url, OidcLoginTransaction), AuthError> {
        let state = SecretToken::generate()?;
        let browser_binding = SecretToken::generate()?;
        let nonce = SecretToken::generate()?.encode();
        let verifier_secret = SecretToken::generate()?.encode();
        let verifier = PkceCodeVerifier::new(verifier_secret.clone());
        let challenge = PkceCodeChallenge::from_code_verifier_sha256(&verifier);
        let state_value = state.encode();
        let nonce_value = nonce.clone();
        let client = self.inner.client.read().await;
        let (url, _, _) = client
            .authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                move || CsrfToken::new(state_value),
                move || Nonce::new(nonce_value),
            )
            .add_scope(Scope::new(String::from("profile")))
            .set_pkce_challenge(challenge)
            .url();

        Ok((
            url,
            OidcLoginTransaction::new(state, browser_binding, nonce, verifier_secret),
        ))
    }

    pub(crate) async fn exchange(
        &self,
        code: String,
        login: &ConsumedOidcLogin,
    ) -> Result<VerifiedIdentity, OidcError> {
        let client = self.inner.client.read().await.clone();
        let observed_generation = self.inner.refresh.lock().await.generation;
        let response = client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| OidcError::Provider)?
            .set_pkce_verifier(PkceCodeVerifier::new(login.pkce_verifier().to_owned()))
            .request_async(&self.inner.http)
            .await
            .map_err(|_| OidcError::Provider)?;
        let id_token = response
            .id_token()
            .ok_or(OidcError::MissingIdToken)?
            .clone();
        let nonce = Nonce::new(login.nonce().to_owned());

        let claims = match id_token.claims(&client.id_token_verifier(), &nonce) {
            Ok(claims) => claims.clone(),
            Err(_) => {
                tracing::warn!(
                    "OIDC ID token validation failed; refreshing provider metadata once"
                );
                self.refresh(observed_generation).await?;
                let refreshed = self.inner.client.read().await;
                id_token
                    .claims(&refreshed.id_token_verifier(), &nonce)
                    .map_err(|_| OidcError::InvalidIdToken)?
                    .clone()
            }
        };

        let profile_name = claims
            .name()
            .and_then(|names| names.get(None))
            .map(|name| name.as_str());
        VerifiedIdentity::new(
            claims.issuer().as_str(),
            claims.subject().as_str(),
            profile_name,
        )
        .map_err(OidcError::Identity)
    }

    async fn refresh(&self, observed_generation: u64) -> Result<(), OidcError> {
        let mut refresh = self.inner.refresh.lock().await;
        if refresh.generation != observed_generation
            || refresh
                .last_refresh
                .is_some_and(|last| last.elapsed() < MIN_FORCED_REFRESH_INTERVAL)
        {
            return Ok(());
        }
        let refreshed = discover_client(
            &self.inner.config,
            &self.inner.redirect_url,
            &self.inner.http,
        )
        .await?;
        *self.inner.client.write().await = refreshed;
        refresh.generation = refresh.generation.wrapping_add(1);
        refresh.last_refresh = Some(Instant::now());
        Ok(())
    }
}

async fn discover_client(
    config: &OidcConfig,
    redirect_url: &RedirectUrl,
    http: &reqwest::Client,
) -> Result<ConfiguredClient, OidcError> {
    let issuer = IssuerUrl::new(config.issuer().to_string())
        .map_err(|error| OidcError::Configuration(error.to_string()))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, http)
        .await
        .map_err(|error| OidcError::Discovery(error.to_string()))?;
    let secret = config
        .client_secret()
        .map(|value| ClientSecret::new(value.to_owned()));

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(config.client_id().to_owned()),
        secret,
    )
    .set_redirect_uri(redirect_url.clone());
    if client.token_uri().is_none() {
        return Err(OidcError::Configuration(String::from(
            "provider metadata has no token endpoint",
        )));
    }
    Ok(client)
}

#[derive(Debug)]
pub(crate) enum OidcError {
    Configuration(String),
    Discovery(String),
    Http(String),
    Identity(AuthError),
    InvalidIdToken,
    MissingIdToken,
    Provider,
}

impl fmt::Display for OidcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(error) => {
                write!(formatter, "OIDC configuration is invalid: {error}")
            }
            Self::Discovery(error) => write!(formatter, "OIDC provider discovery failed: {error}"),
            Self::Http(error) => {
                write!(formatter, "OIDC HTTP client could not be created: {error}")
            }
            Self::Identity(error) => write!(formatter, "OIDC identity claims are invalid: {error}"),
            Self::InvalidIdToken => formatter.write_str("OIDC ID token validation failed"),
            Self::MissingIdToken => {
                formatter.write_str("OIDC response did not contain an ID token")
            }
            Self::Provider => formatter.write_str("OIDC provider rejected the code exchange"),
        }
    }
}

impl std::error::Error for OidcError {}
