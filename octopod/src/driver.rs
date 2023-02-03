use std::net::IpAddr;

use anyhow::Context;
use futures::{Stream, StreamExt};
use maplit::hashmap;
use podman_api::{
    opts::{
        ContainerCreateOpts, ContainerDeleteOpts, ContainerLogsOpts, NetworkConnectOpts,
        NetworkCreateOpts,
    },
    Podman,
};
use uuid::Uuid;

use crate::{emitter::LogLine, resource::Resources, Network, Service, ServiceConfig};

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
            name: config.name.clone(),
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
            .delete(
                &ContainerDeleteOpts::builder()
                    .force(true)
                    .timeout(0)
                    .build(),
            )
            .await?;

        Ok(())
    }

    pub(crate) fn logs(&self, service: &Service) -> impl Stream<Item = LogLine> {
        let name = service.name.clone();
        let container = self.api.containers().get(&service.id);
        let (snd, recv) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut stream = container.logs(
                &ContainerLogsOpts::builder()
                    .stderr(true)
                    .stdout(true)
                    .follow(true)
                    .build(),
            );

            while let Some(chunk) = stream.next().await {
                let data = match chunk.unwrap() {
                    podman_api::conn::TtyChunk::StdOut(data) => data,
                    podman_api::conn::TtyChunk::StdErr(data) => data,
                    _ => Vec::new(),
                };
                let line = LogLine {
                    name: name.clone(),
                    data: String::from_utf8(data).unwrap(),
                };

                if let Err(_) = snd.send(line) {
                    break;
                }
            }
        });

        tokio_stream::wrappers::UnboundedReceiverStream::new(recv)
    }

    pub(crate) async fn disconnect(&self, service: &Service) -> anyhow::Result<()> {
        self.api
            .containers()
            .get(&service.id)
            .disconnect(&service.net.name, true)
            .await?;

        Ok(())
    }

    pub(crate) async fn connect(&self, service: &Service) -> anyhow::Result<()> {
        self.api
            .containers()
            .get(&service.id)
            .connect(
                &service.net.name,
                &NetworkConnectOpts::builder()
                    .aliases([&service.name])
                    .build(),
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn pause(&self, service: &Service) -> anyhow::Result<()> {
        self.api.containers().get(&service.id).pause().await?;
        Ok(())
    }

    pub(crate) async fn unpause(&self, service: &Service) -> anyhow::Result<()> {
        self.api.containers().get(&service.id).unpause().await?;
        Ok(())
    }
}
