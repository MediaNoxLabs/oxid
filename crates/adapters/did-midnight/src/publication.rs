// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
    time::Duration,
};

use oxid_identity_application::{
    DidPublicationPort, DidPublicationPortError, DidPublicationPortFuture,
};
use oxid_identity_domain::DidResolution;
use reqwest::{
    Certificate, Client, StatusCode, Url,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use zeroize::Zeroizing;

use crate::resolution_to_json_value;

const CAPABILITY_BYTES: usize = 64;
const CAPABILITY_FILE: &str = "portal-holder.capability";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Explicit, compile-gated adapter that shares a public DID Resolution Result
/// with the current Portal test issuer. It never receives private key material.
#[derive(Clone)]
pub struct PortalTailnetDidPublisher {
    endpoint: Url,
    client: Client,
    capability_path: PathBuf,
}

impl PortalTailnetDidPublisher {
    pub fn new(public_origin: &str) -> Result<Self, DidPublicationPortError> {
        let endpoint = publication_endpoint(public_origin)?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|certificate| Certificate::from_der(certificate.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| DidPublicationPortError::InvalidConfiguration)?;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .user_agent("oxid-portal-holder-publication/0.1")
            .tls_certs_only(roots)
            .build()
            .map_err(|_| DidPublicationPortError::InvalidConfiguration)?;
        Ok(Self {
            endpoint,
            client,
            capability_path: PathBuf::from("/data/data/io.medianox.oxid/files")
                .join(CAPABILITY_FILE),
        })
    }
}

impl DidPublicationPort for PortalTailnetDidPublisher {
    fn publish<'a>(&'a self, resolution: DidResolution) -> DidPublicationPortFuture<'a> {
        Box::pin(async move {
            let capability = read_capability(&self.capability_path)?;
            let mut header_bytes = Zeroizing::new(Vec::with_capacity(7 + capability.len()));
            header_bytes.extend_from_slice(b"Bearer ");
            header_bytes.extend_from_slice(&capability);
            let mut authorization = HeaderValue::from_bytes(&header_bytes)
                .map_err(|_| DidPublicationPortError::InvalidCapability)?;
            authorization.set_sensitive(true);
            let response = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, authorization)
                .json(&resolution_to_json_value(&resolution))
                .send()
                .await
                .map_err(|_| DidPublicationPortError::Unavailable)?;
            match response.status() {
                StatusCode::OK => {
                    fs::remove_file(&self.capability_path)
                        .map_err(|_| DidPublicationPortError::InvalidCapability)?;
                    Ok(())
                }
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    let _ = fs::remove_file(&self.capability_path);
                    Err(DidPublicationPortError::InvalidCapability)
                }
                _ => Err(DidPublicationPortError::Rejected),
            }
        })
    }
}

fn publication_endpoint(public_origin: &str) -> Result<Url, DidPublicationPortError> {
    if public_origin.len() > 512 {
        return Err(DidPublicationPortError::InvalidConfiguration);
    }
    let origin =
        Url::parse(public_origin).map_err(|_| DidPublicationPortError::InvalidConfiguration)?;
    let host = origin
        .host_str()
        .ok_or(DidPublicationPortError::InvalidConfiguration)?;
    let labels_are_canonical = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    if origin.scheme() != "https"
        || origin.port().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
        || !host.ends_with(".ts.net")
        || host == "ts.net"
        || !labels_are_canonical
        || origin.origin().ascii_serialization() != public_origin
    {
        return Err(DidPublicationPortError::InvalidConfiguration);
    }
    origin
        .join("holder")
        .map_err(|_| DidPublicationPortError::InvalidConfiguration)
}

fn read_capability(path: &Path) -> Result<Zeroizing<Vec<u8>>, DidPublicationPortError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| DidPublicationPortError::InvalidCapability)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() != CAPABILITY_BYTES as u64
    {
        let _ = fs::remove_file(path);
        return Err(DidPublicationPortError::InvalidCapability);
    }
    let capability =
        Zeroizing::new(fs::read(path).map_err(|_| DidPublicationPortError::InvalidCapability)?);
    if capability.len() != CAPABILITY_BYTES
        || !capability
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        let _ = fs::remove_file(path);
        return Err(DidPublicationPortError::InvalidCapability);
    }
    Ok(capability)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_endpoint_accepts_only_canonical_explicit_port_tailnet_origins() {
        assert_eq!(
            publication_endpoint("https://wallet-demo.example.ts.net:10443")
                .expect("canonical Tailnet origin")
                .as_str(),
            "https://wallet-demo.example.ts.net:10443/holder"
        );
        for rejected in [
            "http://wallet-demo.example.ts.net:10443",
            "https://wallet-demo.example.ts.net",
            "https://wallet-demo.example.ts.net:10443/path",
            "https://Wallet-demo.example.ts.net:10443",
            "https://example.com:10443",
        ] {
            assert_eq!(
                publication_endpoint(rejected),
                Err(DidPublicationPortError::InvalidConfiguration),
                "{rejected}"
            );
        }
    }
}
