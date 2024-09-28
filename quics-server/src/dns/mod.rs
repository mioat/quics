use std::io::{Error, Result};
use std::net::SocketAddr;
use std::sync::Arc;

use hickory_resolver::TokioAsyncResolver;

#[derive(Clone)]
pub struct Resolver(Arc<TokioAsyncResolver>);

impl Default for Resolver {
    fn default() -> Self {
        use hickory_resolver::config::{ResolverConfig, ResolverOpts};

        Self(Arc::new(TokioAsyncResolver::tokio(
            ResolverConfig::default(),
            ResolverOpts::default(),
        )))
    }
}

impl quics_protocol::Resolver for Resolver {
    async fn lookup(&self, domain: &str, port: u16) -> Result<SocketAddr> {
        let response = self.0.lookup_ip(domain).await?;
        let address = response
            .iter()
            .next()
            .ok_or_else(|| Error::other(format!("could not resolve domain '{}'", domain)))?;

        Ok(SocketAddr::new(address, port))
    }
}
