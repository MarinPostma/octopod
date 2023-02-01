#[doc(hidden)]
pub mod sealed;

mod emitter;

use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::Context;
use emitter::{Emitter, TestResult};
use podman_api::opts::{ContainerCreateOpts, NetworkCreateOpts};
use podman_api::Podman;
use sealed::{TestDecl, TestFn};
use uuid::Uuid;

pub use octopod_macros::test;

pub struct Octopod {
    driver: Driver,
    suites: Vec<TestSuite>,
}

impl Octopod {
    /// Initialize Octopod, sets up the connection to the podman API, and collects all tests.
    /// An error is returned if an app is used within a test, and is not registered on
    /// initialization.
    pub fn init(podman_addr: &str, apps: Vec<AppConfig>) -> anyhow::Result<Self> {
        let mut suites: HashMap<String, TestSuite> = HashMap::new();
        for config in apps {
            let name = config.name.clone();
            let suite = TestSuite::new(config.clone());
            suites.insert(name, suite);
        }

        for decl in inventory::iter::<TestDecl>() {
            let test = Test {
                f: decl.f,
                name: decl.name.into(),
            };

            suites
                .get_mut(decl.app)
                .with_context(|| format!("unknown app `{}` in test `{}`", decl.app, decl.name))?
                .tests
                .push(test);
        }

        let suites = suites.into_values().collect();
        let driver = Driver::new(podman_addr)?;

        Ok(Self { driver, suites })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let mut emitter = Emitter::default();
        for suite in self.suites {
            suite.run(&self.driver, &mut emitter).await?;
        }

        Ok(())
    }
}

struct Test {
    f: &'static dyn TestFn,
    name: String,
}

struct TestSuite {
    app: AppConfig,
    tests: Vec<Test>,
}

impl TestSuite {
    fn new(app: AppConfig) -> Self {
        Self {
            app,
            tests: Vec::new(),
        }
    }

    async fn instantiate_app(&self, driver: &Driver) -> anyhow::Result<App> {
        let network = driver.network().await?;
        let mut services = HashMap::new();
        for config in &self.app.services {
            let service = driver.service(config, &network).await?;
            services.insert(config.name.clone(), service);
        }

        Ok(App { services, network })
    }

    async fn run(self, driver: &Driver, emitter: &mut Emitter) -> anyhow::Result<()> {
        for Test { name, f } in &self.tests {
            let app = self.instantiate_app(driver).await?;
            let net = app.network.clone();
            let fut = f.call(app);
            let result = match tokio::spawn(fut).await {
                Ok(_) => TestResult::pass(name),
                Err(e) => TestResult::fail(name, &e),
            };
            emitter.emit(result);
            // destroying network will remove all associated containers.
            driver.destroy_network(net).await?;
        }

        Ok(())
    }
}

pub struct Driver {
    api: Podman,
}

#[derive(Clone, Debug)]
struct Network {
    name: String,
}

impl Network {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Driver {
    pub fn new(addr: &str) -> anyhow::Result<Self> {
        let api = Podman::new(addr)?;
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
        let resp = self.api.containers().create(&opts).await?;
        let container = self.api.containers().get(&resp.id);
        container.start(None).await?;
        let meta = container.inspect().await?;
        // TODO: error handling
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
    pub fn service(&self, service: &str) -> Option<&Service> {
        self.services.get(service)
    }
}

#[derive(Clone, Debug)]
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
#[allow(dead_code)]
pub struct Service {
    ip: IpAddr,
    id: String,
}

impl Service {
    /// Ip of this service
    pub fn ip(&self) -> IpAddr {
        self.ip
    }
}
