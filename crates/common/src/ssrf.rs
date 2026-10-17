use std::net::{IpAddr, ToSocketAddrs};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum SsrfError {
    #[error("url must use https")]
    HttpsRequired,
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("hostname resolves to a private/internal IP address")]
    PrivateIpBlocked,
    #[error("hostname resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("url targets a blocked address: {0}")]
    BlockedTarget(String),
}

fn is_private_or_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(addr) => {
            addr.is_loopback()
                || addr.is_private()
                || addr.is_link_local()
                || addr.is_unspecified()
                || addr.is_broadcast()
                || addr.octets()[0] == 100 && (addr.octets()[1] & 0xc0) == 64 // 100.64.0.0/10 (CGN)
        }
        IpAddr::V6(addr) => {
            addr.is_loopback()
                || addr.is_unspecified()
                || addr.is_multicast()
                || (addr.segments()[0] & 0xfe00) == 0xfc00  // fc00::/7 (ULA)
                || (addr.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 (link-local)
        }
    }
}

pub fn validate_url(input: &str) -> Result<(), SsrfError> {
    let url = Url::parse(input).map_err(|e| SsrfError::InvalidUrl(e.to_string()))?;

    if url.scheme() != "https" {
        return Err(SsrfError::HttpsRequired);
    }

    let host = url
        .host_str()
        .ok_or_else(|| SsrfError::InvalidUrl("missing host".into()))?;

    if host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(SsrfError::BlockedTarget(host.to_string()));
    }

    if host == "169.254.169.254" {
        return Err(SsrfError::BlockedTarget("aws metadata endpoint".into()));
    }

    if host == "metadata.google.internal" || host.ends_with(".metadata.google.internal") {
        return Err(SsrfError::BlockedTarget("gcp metadata endpoint".into()));
    }

    // Resolve and check IP addresses
    let port = url.port().unwrap_or(443);
    let addr = format!("{host}:{port}");
    let sockets: Vec<_> = addr
        .to_socket_addrs()
        .map_err(|e| SsrfError::ResolutionFailed(e.to_string()))?
        .collect();

    for socket in &sockets {
        if is_private_or_internal(socket.ip()) {
            return Err(SsrfError::PrivateIpBlocked);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost() {
        assert!(validate_url("https://localhost:8080/webhook").is_err());
        assert!(validate_url("https://127.0.0.1:8080/webhook").is_err());
        assert!(validate_url("https://[::1]:8080/webhook").is_err());
    }

    #[test]
    fn blocks_private_ips() {
        assert!(validate_url("https://10.0.0.1:8080/webhook").is_err());
        assert!(validate_url("https://192.168.1.1:8080/webhook").is_err());
        assert!(validate_url("https://172.16.0.1:8080/webhook").is_err());
    }

    #[test]
    fn blocks_aws_metadata() {
        assert!(validate_url("https://169.254.169.254/latest/meta-data").is_err());
    }

    #[test]
    fn blocks_http() {
        assert!(validate_url("http://example.com/webhook").is_err());
    }

    #[test]
    fn allows_public_https() {
        assert!(validate_url("https://api.example.com/webhook").is_ok());
    }
}
