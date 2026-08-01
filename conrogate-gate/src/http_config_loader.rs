//! HTTP 配置加载器：分离模式下 gate 从 control HTTP API 拉取配置快照。

use conrogate_contract::dto::{RouteDto, UpstreamDto, PluginBindingDto};
use conrogate_contract::ConrogateError;
use http_body_util::BodyExt;

/// HTTP 配置加载器：从 control API 拉取配置
pub struct HttpConfigLoader {
    base_url: String,
    token: String,
    client: hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        http_body_util::Full<bytes::Bytes>,
    >,
}

impl HttpConfigLoader {
    pub fn new(base_url: &str, token: &str) -> Self {
        let client = hyper_util::client::legacy::Client::builder(
            hyper_util::rt::TokioExecutor::new(),
        )
        .build(hyper_util::client::legacy::connect::HttpConnector::new());
        Self {
            base_url: base_url.to_string(),
            token: token.to_string(),
            client,
        }
    }

    fn auth_request(&self, method: &str, path: &str) -> http::Request<http_body_util::Full<bytes::Bytes>> {
        http::Request::builder()
            .method(method)
            .uri(format!("{}{}", self.base_url, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .body(http_body_util::Full::new(bytes::Bytes::new()))
            .unwrap()
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ConrogateError> {
        let req = self.auth_request("GET", path);
        let resp = self.client.request(req).await
            .map_err(|e| ConrogateError::ConfigLoad(format!("HTTP request failed: {e}")))?;

        let status = resp.status();
        let body = resp.into_body().collect().await
            .map_err(|e| ConrogateError::ConfigLoad(format!("body read failed: {e}")))?
            .to_bytes();

        let json: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| ConrogateError::ConfigLoad(format!("JSON parse failed: {e}")))?;

        if !status.is_success() {
            return Err(ConrogateError::ConfigLoad(format!(
                "control API returned {}: {}",
                status,
                json.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown")
            )));
        }

        Ok(json)
    }

    /// 拉取全量路由
    pub async fn load_routes(&self) -> Result<Vec<RouteDto>, ConrogateError> {
        let json = self.get_json("/api/v1/routes?page=1&page_size=200").await?;
        let data = json.get("data").ok_or_else(|| ConrogateError::ConfigLoad("missing data field".into()))?;
        let list = data.get("list").or_else(|| data.as_array().map(|_| data)).ok_or_else(|| ConrogateError::ConfigLoad("missing list field".into()))?;
        serde_json::from_value(list.clone())
            .map_err(|e| ConrogateError::ConfigLoad(format!("route deserialize failed: {e}")))
    }

    /// 拉取全量上游
    pub async fn load_upstreams(&self) -> Result<Vec<UpstreamDto>, ConrogateError> {
        let json = self.get_json("/api/v1/upstreams?page=1&page_size=200").await?;
        let data = json.get("data").ok_or_else(|| ConrogateError::ConfigLoad("missing data field".into()))?;
        let list = data.get("list").or_else(|| data.as_array().map(|_| data)).ok_or_else(|| ConrogateError::ConfigLoad("missing list field".into()))?;
        serde_json::from_value(list.clone())
            .map_err(|e| ConrogateError::ConfigLoad(format!("upstream deserialize failed: {e}")))
    }

    /// 拉取路由的插件绑定
    pub async fn load_plugin_bindings(&self, route_id: u64) -> Result<Vec<PluginBindingDto>, ConrogateError> {
        let json = self.get_json(&format!("/api/v1/routes/{}/plugins", route_id)).await?;
        let data = json.get("data").ok_or_else(|| ConrogateError::ConfigLoad("missing data field".into()))?;
        serde_json::from_value(data.clone())
            .map_err(|e| ConrogateError::ConfigLoad(format!("plugin bindings deserialize failed: {e}")))
    }

    /// 拉取全量插件绑定
    pub async fn load_all_plugin_bindings(&self, routes: &[RouteDto]) -> Result<Vec<PluginBindingDto>, ConrogateError> {
        let mut all = Vec::new();
        for route in routes {
            if let Ok(bindings) = self.load_plugin_bindings(route.id).await {
                all.extend(bindings);
            }
        }
        Ok(all)
    }
}
