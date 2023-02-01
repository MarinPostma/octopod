#[doc(hidden)]
pub mod sealed;

use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;

use podman_api::opts::{ContainerCreateOpts, NetworkCreateOpts};
use podman_api::Podman;
use uuid::Uuid;

pub use octopod_macros::test;

pub struct Orchestrator {
    driver: Driver,
    test_suites: Vec<TestSuite>,
}

impl Orchestrator {
    pub fn new(driver: Driver) -> Self {
        Self {
            driver,
            test_suites: Vec::new(),
        }
    }
    /// Registers a test suite with this orchestrator
    pub fn test_suite<'a>(&'a mut self, app: AppConfig) -> TestSuiteBuilder<'a> {
        TestSuiteBuilder {
            app,
            tests: Vec::new(),
            orchestrator: self,
        }
    }

    pub async fn start(self) -> anyhow::Result<()> {
        dbg!(self.test_suites.len());
        for suite in self.test_suites {
            suite.run(&self.driver).await?;
        }

        Ok(())
    }
}

pub struct TestSuiteBuilder<'a> {
    app: AppConfig,
    tests: Vec<Test>,
    orchestrator: &'a mut Orchestrator,
}

impl<'a> TestSuiteBuilder<'a> {
    pub fn test(
        mut self,
        name: String,
        test: impl Fn(&App) -> Pin<Box<dyn Future<Output = ()> + Sync + Send>> + 'static,
    ) -> Self {
        self.tests.push(Test {
            f: Box::new(test),
            name,
        });
        self
    }

    pub fn build(self) {
        if !self.tests.is_empty() {
            self.orchestrator.test_suites.push(TestSuite {
                app: self.app,
                tests: self.tests,
            });
        }
    }
}

struct Test {
    f: Box<dyn Fn(&App) -> Pin<Box<dyn Future<Output = ()> + Send + Sync>>>,
    name: String,
}

struct TestSuite {
    app: AppConfig,
    tests: Vec<Test>,
}

impl TestSuite {
    async fn instantiate_app(&self, driver: &Driver) -> anyhow::Result<App> {
        dbg!();
        let network = driver.network().await?;
        dbg!();
        let mut services = HashMap::new();
        for config in &self.app.services {
            dbg!();
            let service = driver.service(config, &network).await?;
            services.insert(config.name.clone(), service);
        }
        dbg!();

        Ok(App { services, network })
    }

    async fn run(self, driver: &Driver) -> anyhow::Result<()> {
        for Test { name, f } in &self.tests {
            dbg!(&name);
            let app = self.instantiate_app(driver).await?;
            let fut = f(&app);
            let res = tokio::spawn(fut).await;
            if let Err(e) = res {
                println!("{name} failed: {e}");
            }
            app.destroy(driver).await?;
        }

        Ok(())
    }
}

pub struct Driver {
    api: Podman,
}

struct Network {
    name: String,
}

impl Network {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Driver {
    pub fn new() -> anyhow::Result<Self> {
        let api = Podman::new("unix:///run/podman/podman.sock")?;
        Ok(Self { api })
    }

    async fn network(&self) -> anyhow::Result<Network> {
        let name = Uuid::new_v4().to_string();
        let opts = NetworkCreateOpts::builder()
            .name(&name)
            .dns_enabled(true)
            .build();
        self.api.networks().create(&opts).await?;

        Ok(Network { name })
    }

    async fn service(&self, config: &ServiceConfig, net: &Network) -> anyhow::Result<Service> {
        let opts = ContainerCreateOpts::builder()
            .networks([(net.name(), ())])
            .image(&config.image)
            .env(config.env.clone())
            .build();
        dbg!(&opts);
        let resp = self.api.containers().create(&opts).await?;
        dbg!();
        let container = self.api.containers().get(&resp.id);
        container.start(None).await?;
        let meta = container.inspect().await?;
        dbg!();
        let ip = meta
            .network_settings
            .unwrap()
            .networks
            .unwrap()
            .get(net.name())
            .unwrap()
            .ip_address
            .as_ref()
            .unwrap()
            .parse()?;

        dbg!();
        Ok(Service { id: resp.id, ip })
    }

    async fn destroy_network(&self, network: Network) -> anyhow::Result<()> {
        // remove destroy all the containers associated with the network as well
        self.api.networks().get(network.name()).remove().await?;
        Ok(())
    }
}

pub struct App {
    network: Network,
    services: HashMap<String, Service>,
}

impl App {
    async fn destroy(self, driver: &Driver) -> anyhow::Result<()> {
        driver.destroy_network(self.network).await?;

        Ok(())
    }

    pub fn dns_lookup(&self, service: &str) -> Option<IpAddr> {
        self.services.get(service).map(|s| s.ip)
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppConfig {
    name: String,
    services: Vec<ServiceConfig>,
}

impl AppConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            services: Vec::new(),
        }
    }

    pub fn add_service(&mut self, config: ServiceConfig) {
        self.services.push(config);
    }
}

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub name: String,
    pub image: String,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Service {
    ip: IpAddr,
    id: String,
}
