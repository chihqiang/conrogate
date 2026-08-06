//! Conrogate 官方鉴权插件：JWT Bearer Token 校验。
//!
//! 支持：
//! - HS256/HS384/HS512 对称签名（HMAC）
//! - RS256/RS384/RS512 非对称签名（RSA PEM 静态密钥）
//! - RS256 JWKS 远程密钥集拉取（带缓存 + TTL）
//! - issuer/audience 校验 + 过期检查

use async_trait::async_trait;
use conrogate_core::contract::{
    plugin::{Plugin, PluginContext, PluginKind, PluginOutcome},
    protocol::ProtocolId,
    ConrogateError,
};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Auth 插件配置
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthPluginConfig {
    /// JWT 签名算法：HS256（默认）或 RS256
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    /// HMAC 对称密钥（algorithm=HS256 时必填）
    #[serde(default)]
    pub secret: String,
    /// RSA 公钥 PEM（algorithm=RS256 + 静态密钥时使用）
    #[serde(default)]
    pub rsa_pem: Option<String>,
    /// JWKS 远程密钥集 URL（algorithm=RS256 + 动态密钥时使用）
    #[serde(default)]
    pub jwks_url: Option<String>,
    /// JWKS 缓存 TTL（秒），默认 300
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl_seconds: u64,
    /// 签发者（issuer）
    #[serde(default)]
    pub issuer: Option<String>,
    /// 受众（audience）
    #[serde(default)]
    pub audience: Option<String>,
    /// 是否强制要求 token
    #[serde(default = "default_true")]
    pub require_token: bool,
}

fn default_algorithm() -> String {
    "HS256".into()
}

fn default_jwks_cache_ttl() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

impl Default for AuthPluginConfig {
    fn default() -> Self {
        Self {
            algorithm: "HS256".into(),
            secret: String::new(),
            rsa_pem: None,
            jwks_url: None,
            jwks_cache_ttl_seconds: 300,
            issuer: None,
            audience: None,
            require_token: true,
        }
    }
}

/// JWKS 密钥缓存
struct JwksCache {
    /// kid → DecodingKey
    keys: HashMap<String, DecodingKey>,
    /// 上次拉取时间
    fetched_at: Option<std::time::Instant>,
}

impl JwksCache {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
            fetched_at: None,
        }
    }

    fn is_stale(&self, ttl: std::time::Duration) -> bool {
        match self.fetched_at {
            Some(t) => t.elapsed() > ttl,
            None => true,
        }
    }
}

pub struct AuthPlugin {
    config: Arc<RwLock<AuthPluginConfig>>,
    jwks_cache: Arc<RwLock<JwksCache>>,
}

impl AuthPlugin {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AuthPluginConfig::default())),
            jwks_cache: Arc::new(RwLock::new(JwksCache::new())),
        }
    }

    /// 从配置创建
    pub fn with_config(config: AuthPluginConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            jwks_cache: Arc::new(RwLock::new(JwksCache::new())),
        }
    }

    /// 解析算法枚举
    fn parse_algorithm(alg: &str) -> Result<Algorithm, ConrogateError> {
        match alg.to_uppercase().as_str() {
            "HS256" => Ok(Algorithm::HS256),
            "HS384" => Ok(Algorithm::HS384),
            "HS512" => Ok(Algorithm::HS512),
            "RS256" => Ok(Algorithm::RS256),
            "RS384" => Ok(Algorithm::RS384),
            "RS512" => Ok(Algorithm::RS512),
            other => Err(ConrogateError::PluginConfigInvalid(format!(
                "unsupported algorithm: {}",
                other
            ))),
        }
    }

    /// 判断是否为 HMAC 算法
    fn is_hmac(alg: &str) -> bool {
        matches!(alg.to_uppercase().as_str(), "HS256" | "HS384" | "HS512")
    }

    /// 校验 AuthPluginConfig（validate_config 与 configured 共用）
    fn validate(cfg: &AuthPluginConfig) -> Result<(), ConrogateError> {
        // 检查算法合法性
        let _alg = Self::parse_algorithm(&cfg.algorithm)?;

        if Self::is_hmac(&cfg.algorithm) {
            // HMAC 算法需要 secret
            if cfg.require_token && cfg.secret.is_empty() {
                return Err(ConrogateError::PluginConfigInvalid(
                    "auth plugin: secret is required when require_token=true and algorithm=HS256"
                        .into(),
                ));
            }
        } else {
            // RSA 算法需要 rsa_pem 或 jwks_url
            if cfg.require_token && cfg.rsa_pem.is_none() && cfg.jwks_url.is_none() {
                return Err(ConrogateError::PluginConfigInvalid(
                    "auth plugin: rsa_pem or jwks_url is required when require_token=true and algorithm=RS256"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    /// 获取验证用的 DecodingKey 和 Algorithm
    async fn resolve_key(
        &self,
        header: &jsonwebtoken::Header,
        config: &AuthPluginConfig,
    ) -> Result<(DecodingKey, Algorithm), ConrogateError> {
        let alg = Self::parse_algorithm(&config.algorithm)?;

        if Self::is_hmac(&config.algorithm) {
            // HMAC 对称密钥
            if config.secret.is_empty() {
                return Err(ConrogateError::PluginConfigInvalid(
                    "auth plugin: secret is required for HMAC algorithms".into(),
                ));
            }
            Ok((DecodingKey::from_secret(config.secret.as_bytes()), alg))
        } else {
            // RSA 非对称密钥
            if let Some(ref jwks_url) = config.jwks_url {
                // JWKS 远程密钥集
                let kid = header.kid.as_deref().ok_or_else(|| {
                    ConrogateError::PluginRuntime("JWT header missing 'kid' for JWKS lookup".into())
                })?;

                let ttl = std::time::Duration::from_secs(config.jwks_cache_ttl_seconds);

                // 先尝试从缓存读取（读锁）
                {
                    let cache = self.jwks_cache.read().await;
                    if let Some(key) = cache.keys.get(kid) {
                        return Ok((key.clone(), alg));
                    }
                }

                // 缓存未命中，获取写锁并检查是否需要刷新
                let mut cache = self.jwks_cache.write().await;
                if cache.is_stale(ttl) {
                    match fetch_jwks(jwks_url).await {
                        Ok(keys) => {
                            tracing::info!(
                                jwks_url = %jwks_url,
                                key_count = keys.len(),
                                "JWKS fetched and cached"
                            );
                            cache.keys = keys;
                            cache.fetched_at = Some(std::time::Instant::now());
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "JWKS fetch failed, using stale cache if available"
                            );
                        }
                    }
                }

                // 再次查找
                if let Some(key) = cache.keys.get(kid) {
                    return Ok((key.clone(), alg));
                }

                Err(ConrogateError::PluginRuntime(format!(
                    "JWKS key not found for kid: {}",
                    kid
                )))
            } else if let Some(ref pem) = config.rsa_pem {
                // 静态 RSA PEM
                DecodingKey::from_rsa_pem(pem.as_bytes())
                    .map(|k| (k, alg))
                    .map_err(|e| {
                        ConrogateError::PluginConfigInvalid(format!("invalid RSA PEM: {}", e))
                    })
            } else {
                Err(ConrogateError::PluginConfigInvalid(
                    "auth plugin: rsa_pem or jwks_url is required for RS256".into(),
                ))
            }
        }
    }
}

impl Default for AuthPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// JWT Claims
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct Claims {
    /// 签发者 (iss)
    iss: Option<String>,
    /// 受众 (aud)
    aud: Option<String>,
    /// 过期时间 (exp)
    exp: Option<u64>,
    /// 主题 (sub) — 用户 ID
    sub: Option<String>,
}

/// JWKS 响应格式
#[derive(serde::Deserialize)]
struct JwksResponse {
    keys: Vec<Jwk>,
}

#[derive(serde::Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

/// 从远程 URL 拉取 JWKS
async fn fetch_jwks(url: &str) -> Result<HashMap<String, DecodingKey>, ConrogateError> {
    use http_body_util::BodyExt;

    // 创建 HTTP 客户端（支持 HTTPS）
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| ConrogateError::PluginRuntime(format!("failed to load native roots: {}", e)))?
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(connector);

    // 发送 GET 请求
    let req = http::Request::builder()
        .method("GET")
        .uri(url)
        .header("Accept", "application/json")
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .map_err(|e| ConrogateError::PluginRuntime(format!("JWKS request build failed: {}", e)))?;

    let resp = client
        .request(req)
        .await
        .map_err(|e| ConrogateError::PluginRuntime(format!("JWKS fetch failed: {}", e)))?;

    if !resp.status().is_success() {
        return Err(ConrogateError::PluginRuntime(format!(
            "JWKS fetch returned status: {}",
            resp.status()
        )));
    }

    // 读取响应体
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .map_err(|e| {
            ConrogateError::PluginRuntime(format!("JWKS response body read failed: {}", e))
        })?
        .to_bytes();

    // 解析 JWKS JSON
    let jwks: JwksResponse = serde_json::from_slice(&body_bytes)
        .map_err(|e| ConrogateError::PluginRuntime(format!("JWKS parse failed: {}", e)))?;

    // 转换为 DecodingKey 映射
    let mut keys = HashMap::new();
    for jwk in jwks.keys {
        if jwk.kty != "RSA" {
            continue;
        }
        let kid = match jwk.kid {
            Some(k) => k,
            None => continue,
        };
        let n = match jwk.n {
            Some(ref n) => n.as_str(),
            None => continue,
        };
        let e = match jwk.e {
            Some(ref e) => e.as_str(),
            None => continue,
        };
        match DecodingKey::from_rsa_components(n, e) {
            Ok(key) => {
                keys.insert(kid, key);
            }
            Err(err) => {
                tracing::warn!(
                    kid = %kid,
                    error = %err,
                    "failed to create RSA key from JWK components"
                );
            }
        }
    }

    Ok(keys)
}

#[async_trait]
impl Plugin for AuthPlugin {
    fn name(&self) -> &'static str {
        "auth"
    }

    fn kind(&self) -> PluginKind {
        PluginKind::Native
    }

    fn protocols(&self) -> &[ProtocolId] {
        &[ProtocolId::Http, ProtocolId::WebSocket]
    }

    fn is_blocking(&self) -> bool {
        true
    }

    fn validate_config(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        let cfg: AuthPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Self::validate(&cfg)
    }

    fn configured(&self, config: &Value) -> Result<Arc<dyn Plugin>, ConrogateError> {
        if config.is_null() {
            return Ok(Arc::new(AuthPlugin::new()));
        }
        let cfg: AuthPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        Self::validate(&cfg)?;
        Ok(Arc::new(AuthPlugin::with_config(cfg)))
    }

    async fn init(&self, config: &Value) -> Result<(), ConrogateError> {
        if config.is_null() {
            return Ok(());
        }
        let cfg: AuthPluginConfig = serde_json::from_value(config.clone())
            .map_err(|e| ConrogateError::PluginConfigInvalid(e.to_string()))?;
        *self.config.write().await = cfg;
        Ok(())
    }

    async fn before_request(
        &self,
        ctx: &mut PluginContext,
    ) -> Result<PluginOutcome, ConrogateError> {
        let config = self.config.read().await.clone();
        let require_token = config.require_token;

        // 从 HTTP 头提取 Authorization
        let token = ctx
            .http
            .as_ref()
            .and_then(|h| h.headers.get("authorization"))
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        match (token, require_token) {
            (Some(token), _) => {
                // 解码 JWT header（不验签），获取 kid 和 alg
                let header = match decode_header(&token) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!(
                            trace_id = %ctx.trace_id,
                            error = %e,
                            "JWT header decode failed"
                        );
                        return Ok(PluginOutcome::Terminate(
                            http::StatusCode::UNAUTHORIZED,
                            serde_json::json!({
                                "code": 10002,
                                "msg": format!("unauthorized: invalid token header: {}", e)
                            }),
                        ));
                    }
                };

                // 获取验证密钥
                let (key, alg) = match self.resolve_key(&header, &config).await {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!(
                            trace_id = %ctx.trace_id,
                            error = %e,
                            "JWT key resolution failed"
                        );
                        return Ok(PluginOutcome::Terminate(
                            http::StatusCode::UNAUTHORIZED,
                            serde_json::json!({
                                "code": 10002,
                                "msg": format!("unauthorized: {}", e)
                            }),
                        ));
                    }
                };

                // 构建验证参数
                let mut validation = Validation::new(alg);
                validation.validate_exp = true;

                if let Some(ref iss) = config.issuer {
                    validation.set_issuer(&[iss]);
                }
                if let Some(ref aud) = config.audience {
                    validation.set_audience(&[aud]);
                }

                match decode::<Claims>(&token, &key, &validation) {
                    Ok(_claims) => {
                        // 校验通过，放行
                        Ok(PluginOutcome::Continue)
                    }
                    Err(e) => {
                        tracing::warn!(
                            trace_id = %ctx.trace_id,
                            error = %e,
                            "JWT validation failed"
                        );
                        Ok(PluginOutcome::Terminate(
                            http::StatusCode::UNAUTHORIZED,
                            serde_json::json!({
                                "code": 10002,
                                "msg": format!("unauthorized: {}", e)
                            }),
                        ))
                    }
                }
            }
            (None, true) => {
                // 需要鉴权但无 token
                Ok(PluginOutcome::Terminate(
                    http::StatusCode::UNAUTHORIZED,
                    serde_json::json!({
                        "code": 10002,
                        "msg": "unauthorized: missing bearer token"
                    }),
                ))
            }
            (None, false) => Ok(PluginOutcome::Continue),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrogate_core::contract::plugin::{
        HttpContext, PluginContext, PluginLogger, PluginMetrics, PluginOutcome, PluginServices,
    };
    use conrogate_core::contract::protocol::ProtocolId;
    use http::Method;
    use jsonwebtoken::{encode, EncodingKey, Header};

    struct NoopMetrics;
    #[async_trait]
    impl PluginMetrics for NoopMetrics {
        async fn increment(&self, _name: &str) {}
        async fn gauge(&self, _name: &str, _value: f64) {}
    }
    struct NoopLogger;
    #[async_trait]
    impl PluginLogger for NoopLogger {
        async fn log(&self, _level: &str, _msg: &str) {}
    }

    fn make_ctx(auth_header: Option<&str>) -> PluginContext {
        let mut headers = http::HeaderMap::new();
        if let Some(h) = auth_header {
            headers.insert("authorization", h.parse().unwrap());
        }
        PluginContext {
            request_id: "test-req".into(),
            trace_id: "test-trace".into(),
            route_id: 1,
            client_ip: "127.0.0.1".into(),
            protocol: ProtocolId::Http,
            http: Some(HttpContext {
                method: Method::GET,
                path: "/test".into(),
                query: Default::default(),
                headers,
                body: None,
            }),
            tunnel: None,
            services: PluginServices {
                metrics: Arc::new(NoopMetrics),
                logger: Arc::new(NoopLogger),
            },
        }
    }

    fn make_token(secret: &str, iss: Option<&str>, aud: Option<&str>) -> String {
        let mut claims = serde_json::json!({
            "sub": "test-user",
            "exp": (chrono::Utc::now().timestamp() + 3600) as u64,
        });
        if let Some(iss) = iss {
            claims["iss"] = serde_json::json!(iss);
        }
        if let Some(aud) = aud {
            claims["aud"] = serde_json::json!(aud);
        }
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    /// 生成 RS256 JWT token（用于测试）
    fn make_rs256_token(pem: &str, iss: Option<&str>, aud: Option<&str>) -> String {
        let mut claims = serde_json::json!({
            "sub": "test-user",
            "exp": (chrono::Utc::now().timestamp() + 3600) as u64,
        });
        if let Some(iss) = iss {
            claims["iss"] = serde_json::json!(iss);
        }
        if let Some(aud) = aud {
            claims["aud"] = serde_json::json!(aud);
        }
        let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
        header.kid = Some("test-key-1".to_string());
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
        encode(&header, &claims, &encoding_key).unwrap()
    }

    /// 生成测试用 RSA 密钥对（PEM 格式）
    fn generate_rsa_keypair() -> (String, String) {
        use std::io::Write;
        // 使用 openssl 命令行生成密钥对（测试环境需要有 openssl）
        let mut child = std::process::Command::new("openssl")
            .args(["genrsa", "2048"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("openssl not available");
        let private_pem = {
            let mut buf = String::new();
            if let Some(mut stdout) = child.stdout.take() {
                use std::io::Read;
                stdout.read_to_string(&mut buf).unwrap();
            }
            buf
        };
        let _ = child.wait();

        // 从私钥导出公钥
        let mut child2 = std::process::Command::new("openssl")
            .args(["rsa", "-pubout"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("openssl not available");
        {
            let mut stdin = child2.stdin.take().unwrap();
            stdin.write_all(private_pem.as_bytes()).unwrap();
        }
        let public_pem = {
            let mut buf = String::new();
            if let Some(mut stdout) = child2.stdout.take() {
                use std::io::Read;
                stdout.read_to_string(&mut buf).unwrap();
            }
            buf
        };
        let _ = child2.wait();

        (private_pem, public_pem)
    }

    #[tokio::test]
    async fn test_valid_token_hs256() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "HS256".into(),
            secret: "test-secret".into(),
            ..Default::default()
        });
        let token = make_token("test-secret", None, None);
        let mut ctx = make_ctx(Some(&format!("Bearer {}", token)));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn test_missing_token() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "HS256".into(),
            secret: "test-secret".into(),
            require_token: true,
            ..Default::default()
        });
        let mut ctx = make_ctx(None);
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn test_invalid_token() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "HS256".into(),
            secret: "test-secret".into(),
            ..Default::default()
        });
        let mut ctx = make_ctx(Some("Bearer invalid-token"));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn test_no_token_not_required() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "HS256".into(),
            secret: "test-secret".into(),
            require_token: false,
            ..Default::default()
        });
        let mut ctx = make_ctx(None);
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn test_issuer_mismatch() {
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "HS256".into(),
            secret: "test-secret".into(),
            issuer: Some("expected-iss".into()),
            ..Default::default()
        });
        let token = make_token("test-secret", Some("wrong-iss"), None);
        let mut ctx = make_ctx(Some(&format!("Bearer {}", token)));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[tokio::test]
    async fn test_valid_token_rs256() {
        let (private_pem, public_pem) = generate_rsa_keypair();
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "RS256".into(),
            rsa_pem: Some(public_pem),
            ..Default::default()
        });
        let token = make_rs256_token(&private_pem, None, None);
        let mut ctx = make_ctx(Some(&format!("Bearer {}", token)));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Continue));
    }

    #[tokio::test]
    async fn test_invalid_token_rs256() {
        let (_private_pem, public_pem) = generate_rsa_keypair();
        let plugin = AuthPlugin::with_config(AuthPluginConfig {
            algorithm: "RS256".into(),
            rsa_pem: Some(public_pem),
            ..Default::default()
        });
        let mut ctx = make_ctx(Some("Bearer invalid-token"));
        let result = plugin.before_request(&mut ctx).await.unwrap();
        assert!(matches!(result, PluginOutcome::Terminate(_, _)));
    }

    #[test]
    fn test_validate_config_empty_secret_hs256() {
        let plugin = AuthPlugin::new();
        let config = serde_json::json!({"algorithm": "HS256", "secret": "", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_ok_hs256() {
        let plugin = AuthPlugin::new();
        let config =
            serde_json::json!({"algorithm": "HS256", "secret": "my-secret", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_missing_key_rs256() {
        let plugin = AuthPlugin::new();
        let config = serde_json::json!({"algorithm": "RS256", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_ok_rs256_pem() {
        let plugin = AuthPlugin::new();
        let config =
            serde_json::json!({"algorithm": "RS256", "rsa_pem": "fake-pem", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_ok_rs256_jwks() {
        let plugin = AuthPlugin::new();
        let config = serde_json::json!({"algorithm": "RS256", "jwks_url": "https://example.com/.well-known/jwks.json", "require_token": true});
        let result = plugin.validate_config(&config);
        assert!(result.is_ok());
    }
}
