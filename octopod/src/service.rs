use std::net::IpAddr;

use crate::{driver::Driver, Network};

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub(crate) name: String,
    pub(crate) image: String,
    pub(crate) env: Vec<(String, String)>,
    /// Url to health check the service.
    pub(crate) health: Option<(String, u16)>,
}

impl ServiceConfig {
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            env: Vec::new(),
            health: None,
        }
    }

    /// Add environment variables to the service.
    pub fn env(
        mut self,
        env: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.env
            .extend(env.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Set the URL to be checked for health
    /// If set, the octopod will wait for the health route to return success before proceeding to
    /// the tests.
    pub fn health(mut self, uri: impl Into<String>, port: u16) -> Self {
        self.health.replace((uri.into(), port));
        self
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct Service {
    pub(crate) name: String,
    pub(crate) net: Network,
    pub(crate) id: String,
    pub(crate) driver: Driver,
}

impl Service {
    /// Retrieve the IP address of this service.
    pub async fn ip(&self) -> anyhow::Result<IpAddr> {
        self.driver.get_service_ip(self).await
    }

    /// Disconnect this service from the network.
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        self.driver.disconnect(self).await
    }

    /// Connect this service back to its network.
    pub async fn connect(&self) -> anyhow::Result<()> {
        self.driver.connect(self).await
    }

    /// pauses the service
    pub async fn pause(&self) -> anyhow::Result<()> {
        self.driver.pause(self).await
    }

    /// unpauses the service
    pub async fn unpause(&self) -> anyhow::Result<()> {
        self.driver.unpause(self).await
    }
}
