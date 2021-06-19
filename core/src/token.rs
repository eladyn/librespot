// Ported from librespot-java. Relicensed under MIT with permission.

use crate::mercury::MercuryError;

use serde::Deserialize;

use std::error::Error;
use std::time::{Duration, Instant};

component! {
    TokenProvider : TokenProviderInner {
        tokens: Vec<Token> = vec![],
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    expires_in: Duration,
    access_token: String,
    scopes: Vec<String>,
    timestamp: Instant,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenData {
    expires_in: u64,
    access_token: String,
    scope: Vec<String>,
}

impl TokenProvider {
    const KEYMASTER_CLIENT_ID: &'static str = "65b708073fc0480ea92a077233ca87bd";

    fn find_token(&self, scopes: Vec<String>) -> Option<usize> {
        self.lock(|inner| {
            for i in 0..inner.tokens.len() {
                if inner.tokens[i].in_scopes(scopes.clone()) {
                    return Some(i);
                }
            }
            None
        })
    }

    pub async fn get_token(&self, scopes: Vec<String>) -> Result<Token, MercuryError> {
        if scopes.is_empty() {
            return Err(MercuryError);
        }

        if let Some(index) = self.find_token(scopes.clone()) {
            let cached_token = self.lock(|inner| inner.tokens[index].clone());
            if cached_token.is_expired() {
                self.lock(|inner| inner.tokens.remove(index));
            } else {
                return Ok(cached_token);
            }
        }

        trace!(
            "Requested token in scopes {:?} unavailable or expired, requesting new token.",
            scopes
        );

        let query_uri = format!(
            "hm://keymaster/token/authenticated?scope={}&client_id={}&device_id={}",
            scopes.join(","),
            Self::KEYMASTER_CLIENT_ID,
            self.session().device_id()
        );
        let request = self.session().mercury().get(query_uri);
        let response = request.await?;

        if response.status_code == 200 {
            let data = response
                .payload
                .first()
                .expect("No tokens received")
                .to_vec();
            let token = Token::new(String::from_utf8(data).unwrap()).map_err(|_| MercuryError)?;
            trace!("Got token: {:?}", token);
            self.lock(|inner| inner.tokens.push(token.clone()));
            Ok(token)
        } else {
            Err(MercuryError)
        }
    }
}

impl Token {
    const EXPIRY_THRESHOLD: Duration = Duration::from_secs(10);

    pub fn new(body: String) -> Result<Self, Box<dyn Error>> {
        let data: TokenData = serde_json::from_slice(body.as_ref())?;
        Ok(Self {
            expires_in: Duration::from_secs(data.expires_in),
            access_token: data.access_token,
            scopes: data.scope,
            timestamp: Instant::now(),
        })
    }

    pub fn is_expired(&self) -> bool {
        self.timestamp + (self.expires_in - Self::EXPIRY_THRESHOLD) < Instant::now()
    }

    pub fn in_scope(&self, scope: String) -> bool {
        for s in &self.scopes {
            if *s == scope {
                return true;
            }
        }
        false
    }

    pub fn in_scopes(&self, scopes: Vec<String>) -> bool {
        for s in scopes {
            if !self.in_scope(s) {
                return false;
            }
        }
        true
    }
}
