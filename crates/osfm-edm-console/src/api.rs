//! Blocking HTTP client. Call only from the worker thread.

use std::sync::Mutex;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::model::{Envelope, LoginData, LoginReq};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    Message(String),
    #[error("HTTP {0}: {1}")]
    Http(u16, String),
}

#[derive(Clone)]
pub struct Api {
    inner: std::sync::Arc<Inner>,
}

struct Inner {
    base: String,
    client: Client,
    token: Mutex<Option<String>>,
}

impl Api {
    pub fn new(base: String) -> Result<Self, ApiError> {
        let client = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| ApiError::Message(e.to_string()))?;
        Ok(Self {
            inner: std::sync::Arc::new(Inner {
                base: base.trim_end_matches('/').to_string(),
                client,
                token: Mutex::new(None),
            }),
        })
    }

    pub fn base(&self) -> &str {
        &self.inner.base
    }

    pub fn set_token(&self, token: Option<String>) {
        *self.inner.token.lock().expect("token lock") = token;
    }

    pub fn token(&self) -> Option<String> {
        self.inner.token.lock().expect("token lock").clone()
    }

    pub fn login(&self, user: &str, pass: &str, totp: &str) -> Result<LoginData, ApiError> {
        let totp = totp.trim();
        let body = LoginReq {
            username: user,
            password: pass,
            totp_code: if totp.is_empty() { None } else { Some(totp) },
        };
        let data: LoginData = self.call(Method::POST, "/api/v1/auth/login", Some(&body))?;
        self.set_token(Some(data.access_token.clone()));
        Ok(data)
    }

    pub fn logout(&self) -> Result<(), ApiError> {
        let _ = self.call_empty(Method::POST, "/api/v1/auth/logout")?;
        self.set_token(None);
        Ok(())
    }

    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.call(Method::GET, path, None::<&()>)
    }

    pub fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T, ApiError> {
        self.call(Method::POST, path, Some(body))
    }

    pub fn post_no_body<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.call(Method::POST, path, None::<&()>)
    }

    /// Blocking SSE reader. Sends `data:` payloads to `tx` until the stream ends.
    pub fn stream_sse(&self, path: &str, tx: std::sync::mpsc::Sender<String>) {
        use std::io::Read;
        let url = format!("{}{path}", self.inner.base);
        let mut req = self.inner.client.get(url);
        if let Some(token) = self.token() {
            req = req.bearer_auth(token);
        }
        let mut res = match req.send() {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(format!("[sse error] {e}\n"));
                return;
            }
        };
        if !res.status().is_success() {
            let _ = tx.send(format!("[sse http {}]\n", res.status().as_u16()));
            return;
        }
        let mut buf = String::new();
        let mut chunk = [0u8; 2048];
        loop {
            match res.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    while let Some(idx) = buf.find("\n\n") {
                        let block = buf[..idx].to_string();
                        buf = buf[idx + 2..].to_string();
                        let mut event = "message";
                        let mut data = String::new();
                        for line in block.lines() {
                            if let Some(rest) = line.strip_prefix("event:") {
                                event = rest.trim();
                            } else if let Some(rest) = line.strip_prefix("data:") {
                                data.push_str(rest.trim_start());
                            }
                        }
                        if event == "closed" {
                            let _ = tx.send(format!("\n[session closed {data}]\n"));
                            return;
                        }
                        if !data.is_empty() {
                            let _ = tx.send(data);
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("[sse read] {e}\n"));
                    break;
                }
            }
        }
    }

    pub fn post_empty<B: Serialize>(&self, path: &str, body: Option<&B>) -> Result<(), ApiError> {
        let _: serde_json::Value = match body {
            Some(b) => self.call(Method::POST, path, Some(b))?,
            None => self.call(Method::POST, path, None::<&()>)?,
        };
        Ok(())
    }

    pub fn patch<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T, ApiError> {
        self.call(Method::PATCH, path, Some(body))
    }

    pub fn delete(&self, path: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self.call(Method::DELETE, path, None::<&()>)?;
        Ok(())
    }

    fn call<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        match self.call_once(method.clone(), path, body) {
            Err(ApiError::Http(401, _)) if !path.starts_with("/api/v1/auth/") => {
                if self.refresh() {
                    self.call_once(method, path, body)
                } else {
                    Err(ApiError::Message("session expired — sign in again".into()))
                }
            }
            other => other,
        }
    }

    fn call_empty(&self, method: Method, path: &str) -> Result<(), ApiError> {
        let _: serde_json::Value = self.call(method, path, None::<&()>)?;
        Ok(())
    }

    fn refresh(&self) -> bool {
        match self.call_once::<LoginData, ()>(Method::POST, "/api/v1/auth/refresh", None) {
            Ok(data) => {
                self.set_token(Some(data.access_token));
                true
            }
            Err(_) => {
                self.set_token(None);
                false
            }
        }
    }

    fn call_once<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        let url = format!("{}{path}", self.inner.base);
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(token) = self.token() {
            if let Ok(v) = HeaderValue::from_str(&format!("Bearer {token}")) {
                headers.insert(AUTHORIZATION, v);
            }
        }
        let mut req = self.inner.client.request(method, url).headers(headers);
        if let Some(b) = body {
            req = req.json(b);
        }
        let res = req.send().map_err(|e| ApiError::Message(e.to_string()))?;
        let status = res.status().as_u16();
        let env: Envelope<T> = res.json().map_err(|e| ApiError::Message(e.to_string()))?;
        if !(200..300).contains(&status) || env.error.is_some() || env.data.is_none() {
            let msg = env
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| format!("request failed ({status})"));
            return Err(ApiError::Http(status, msg));
        }
        Ok(env.data.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Envelope;

    #[test]
    fn envelope_ok_round_trip() {
        let raw = r#"{"data":{"access_token":"abc"},"error":null}"#;
        let env: Envelope<LoginData> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.data.unwrap().access_token, "abc");
    }

    #[test]
    fn envelope_err() {
        let raw = r#"{"data":null,"error":{"code":"UNAUTHORIZED","message":"nope"}}"#;
        let env: Envelope<LoginData> = serde_json::from_str(raw).unwrap();
        assert_eq!(env.error.unwrap().code, "UNAUTHORIZED");
    }
}
