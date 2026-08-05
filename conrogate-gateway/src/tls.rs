//! TLS 证书加载与 TlsAcceptor 构建。

use conrogate_contract::config::TlsConfig;
use std::io::Cursor;
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

/// 从配置构建 TlsAcceptor
///
/// - cert_file: PEM 格式证书文件路径
/// - key: PEM 格式私钥内容（直接从环境变量读取）
pub fn build_tls_acceptor(tls_config: &TlsConfig) -> Result<TlsAcceptor, String> {
    // 读取证书文件
    let cert_pem = std::fs::read(&tls_config.cert_file)
        .map_err(|e| format!("read cert file '{}': {}", tls_config.cert_file, e))?;

    // 私钥直接从环境变量值读取（不是文件路径）
    let key_pem = tls_config.key.as_bytes().to_vec();

    // 解析 PEM 证书
    let mut cert_reader = Cursor::new(cert_pem);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("parse cert PEM: {e}"))?;

    if certs.is_empty() {
        return Err("no certificates found in cert file".into());
    }

    // 解析 PEM 私钥（rustls-pemfile v2: private_key 返回 PrivateKeyDer）
    let mut key_reader = Cursor::new(key_pem);
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("parse private key PEM: {e}"))?
        .ok_or_else(|| "no private key found in PEM".to_string())?;

    // 构建 ServerConfig
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("build TLS server config: {e}"))?;

    // ALPN：优先 h2，其次 http/1.1
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let server_config = Arc::new(server_config);
    Ok(TlsAcceptor::from(server_config))
}
