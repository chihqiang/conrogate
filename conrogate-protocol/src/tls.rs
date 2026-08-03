//! TLS 辅助：出站证书校验跳过 + ClientHello SNI 提取（TLS passthrough 路由）。

use std::sync::Arc;

/// 跳过上游证书校验的 ServerCertVerifier（docs/15：仅非生产）。
#[derive(Debug)]
pub struct NoVerifyServerCertVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifyServerCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// 从 TLS ClientHello 首包中提取 SNI（docs/10 §2.3：passthrough 模式按 SNI 路由）。
///
/// 解析失败返回 None（非 TLS 或格式异常），调用方按无 SNI 处理。
pub fn extract_sni_from_client_hello(buf: &[u8]) -> Option<String> {
    // 1. TLS record header：type(1) + version(2) + length(2)
    if buf.len() < 5 || buf[0] != 0x16 {
        return None;
    }
    let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    if buf.len() < 5 + record_len {
        return None;
    }

    // 2. Handshake header：type(1) + length(3)
    let hs = &buf[5..5 + record_len];
    if hs.len() < 4 || hs[0] != 0x01 {
        return None;
    }
    let _hs_len = ((hs[1] as usize) << 16) | ((hs[2] as usize) << 8) | hs[3] as usize;
    let body = &hs[4..];

    // 3. ClientHello body：version(2) + random(32)
    if body.len() < 34 {
        return None;
    }
    let mut off = 34usize;

    // 4. SessionID
    if off + 1 > body.len() {
        return None;
    }
    let session_len = body[off] as usize;
    off += 1 + session_len;
    if off + 2 > body.len() {
        return None;
    }

    // 5. Cipher suites
    let ciphers_len = u16::from_be_bytes([body[off], body[off + 1]]) as usize;
    off += 2 + ciphers_len;
    if off + 1 > body.len() {
        return None;
    }

    // 6. Compression methods
    let comp_len = body[off] as usize;
    off += 1 + comp_len;
    if off + 2 > body.len() {
        return None;
    }

    // 7. Extensions
    let ext_total = u16::from_be_bytes([body[off], body[off + 1]]) as usize;
    off += 2;
    let ext_end = (off + ext_total).min(body.len());
    while off + 4 <= ext_end {
        let ext_type = u16::from_be_bytes([body[off], body[off + 1]]);
        let ext_len = u16::from_be_bytes([body[off + 2], body[off + 3]]) as usize;
        off += 4;
        let data_end = (off + ext_len).min(ext_end);
        if ext_type == 0x0000 {
            // SNI extension：server_name_list length(2) + entries
            let mut p = off;
            if p + 2 > data_end {
                return None;
            }
            let _list_len = u16::from_be_bytes([body[p], body[p + 1]]) as usize;
            p += 2;
            while p + 3 <= data_end {
                let name_type = body[p];
                let name_len = u16::from_be_bytes([body[p + 1], body[p + 2]]) as usize;
                p += 3;
                if p + name_len > data_end {
                    return None;
                }
                if name_type == 0x00 {
                    let name = std::str::from_utf8(&body[p..p + name_len]).ok()?;
                    if !name.is_empty() {
                        return Some(name.to_string());
                    }
                }
                p += name_len;
            }
        }
        off = data_end;
    }
    None
}

/// 便捷构造：带证书校验的 HttpsConnector 客户端（默认）
pub fn no_verify_tls_config() -> Arc<rustls::ClientConfig> {
    let verifier = Arc::new(NoVerifyServerCertVerifier);
    Arc::new(
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个含 SNI 扩展的最小 ClientHello 片段
    fn build_client_hello(sni: &str) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&[0x03, 0x03]); // version TLS1.2
        body.extend_from_slice(&[0u8; 32]); // random
        body.push(0); // session id
        body.extend_from_slice(&[0x00, 0x02]); // cipher suites len
        body.extend_from_slice(&[0x13, 0x01]); // TLS_AES_128_GCM_SHA256
        body.push(1); // compression methods
        body.push(0);
        // SNI extension
        let name = sni.as_bytes();
        let mut ext_data = Vec::new();
        let entry_len = 3 + name.len();
        ext_data.extend_from_slice(&[(entry_len >> 8) as u8, entry_len as u8]);
        ext_data.push(0x00); // host_name
        ext_data.extend_from_slice(&[(name.len() >> 8) as u8, name.len() as u8]);
        ext_data.extend_from_slice(name);
        // extensions 总长度字段
        let ext_block_len = 2 + 2 + ext_data.len();
        body.extend_from_slice(&[(ext_block_len >> 8) as u8, ext_block_len as u8]);
        body.extend_from_slice(&[0x00, 0x00]); // ext type SNI
        body.extend_from_slice(&[(ext_data.len() >> 8) as u8, ext_data.len() as u8]);
        body.extend_from_slice(&ext_data);

        // handshake header
        let mut hs = Vec::new();
        hs.push(0x01); // ClientHello
        hs.extend_from_slice(&[
            ((body.len() >> 16) & 0xff) as u8,
            ((body.len() >> 8) & 0xff) as u8,
            (body.len() & 0xff) as u8,
        ]);
        hs.extend_from_slice(&body);

        // record header
        let mut rec = Vec::new();
        rec.push(0x16);
        rec.extend_from_slice(&[0x03, 0x03]);
        rec.extend_from_slice(&[(hs.len() >> 8) as u8, hs.len() as u8]);
        rec.extend_from_slice(&hs);
        rec
    }

    #[test]
    fn test_extract_sni() {
        let buf = build_client_hello("api.example.com");
        assert_eq!(
            extract_sni_from_client_hello(&buf).as_deref(),
            Some("api.example.com")
        );
    }

    #[test]
    fn test_extract_sni_from_garbage() {
        assert_eq!(extract_sni_from_client_hello(b"not tls at all"), None);
        assert_eq!(extract_sni_from_client_hello(&[]), None);
    }
}
