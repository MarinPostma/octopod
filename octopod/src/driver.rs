use std::net::IpAddr;

use anyhow::Context;
use maplit::hashmap;
use podman_api::{
    opts::{ContainerCreateOpts, ContainerDeleteOpts, NetworkCreateOpts},
    Podman,
};
use uuid::Uuid;

use crate::{resource::Resources, Network, Service, ServiceConfig};

#[derive(Clone)]
pub(crate) struct Driver {
    api: Podman,
}

impl Driver {
    pub fn new(addr: &str) -> anyhow::Result<Self> {
        let api = Podman::new(addr)?;
        Ok(Self { api })
    }

    pub async fn network(&self, resources: &mut Resources) -> anyhow::Result<Network> {
        let name = Uuid::new_v4().to_string();
        let opts = NetworkCreateOpts::builder()
            .name(&name)
            .dns_enabled(true)
            .build();
        self.api.networks().create(&opts).await?;

        let net = Network { name };
        resources.register(net.clone());

        Ok(net)
    }

    pub async fn service(
        &self,
        config: &ServiceConfig,
        net: &Network,
        resources: &mut Resources,
    ) -> anyhow::Result<Service> {
        let opts = ContainerCreateOpts::builder()
            .networks([(net.name(), hashmap! { "aliases" => vec![&config.name]})])
            .image(&config.image)
            .env(config.env.clone())
            .build();
        let resp = self.api.containers().create(&opts).await?;
        let container = self.api.containers().get(&resp.id);
        container.start(None).await?;

        let service = Service {
            id: resp.id,
            net: net.clone(),
            driver: self.clone(),
        };

        resources.register(service.clone());

        Ok(service)
    }

    pub async fn destroy_network(&self, network: &Network) -> anyhow::Result<()> {
        // remove destroy all the containers associated with the network as well
        self.api.networks().get(network.name()).remove().await?;
        Ok(())
    }

    pub async fn get_service_ip(&self, service: &Service) -> anyhow::Result<IpAddr> {
        let container = self.api.containers().get(&service.id);
        let meta = container.inspect().await?;
        // TODO: error handling
        let ip = meta
            .network_settings
            .context("invalid service network config")?
            .networks
            .context("invalid service network config")?
            .get(service.net.name())
            .context("invalid service network config")?
            .ip_address
            .as_ref()
            .context("invalid service network config")?
            .parse()?;

        Ok(ip)
    }

    pub async fn destroy_service(&self, service: &Service) -> anyhow::Result<()> {
        let container = self.api.containers().get(&service.id);
        container
            .delete(&ContainerDeleteOpts::builder().force(true).build())
            .await?;

        Ok(())
    }
}
