#[doc(hidden)]
pub mod sealed;

mod driver;
mod emitter;
mod resource;

use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::Context;
use driver::Driver;
use emitter::{Emitter, TestResult};
use resource::Resources;
use sealed::{TestDecl, TestFn};

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
            let mut resources = Resources::default();
            if let Err(e) = suite.run(&self.driver, &mut emitter, &mut resources).await {
                eprintln!("error running test suite: {e}");
            }

            resources.cleanup(&self.driver).await;
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

    async fn instantiate_app(
        &self,
        driver: &Driver,
        resources: &mut Resources,
    ) -> anyhow::Result<App> {
        let network = driver.network(resources).await?;
        let mut services = HashMap::new();
        for config in &self.app.services {
            let service = driver.service(config, &network, resources).await?;
            services.insert(config.name.clone(), service);
        }

        Ok(App { services })
    }

    async fn run(
        self,
        driver: &Driver,
        emitter: &mut Emitter,
        resources: &mut Resources,
    ) -> anyhow::Result<()> {
        for Test { name, f } in &self.tests {
            let app = self.instantiate_app(driver, resources).await?;
            let fut = f.call(app);
            //FIXME: Maybe we should fork here, and collect stdout
            let result = match tokio::spawn(fut).await {
                Ok(_) => TestResult::pass(name),
                Err(e) => TestResult::fail(name, &e),
            };
            emitter.emit(result);
        }

        Ok(())
    }
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

pub struct App {
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

#[derive(Clone)]
#[allow(dead_code)]
pub struct Service {
    net: Network,
    id: String,
    driver: Driver,
}

impl Service {
    /// Ip of this service
    pub async fn ip(&self) -> anyhow::Result<IpAddr> {
        self.driver.get_service_ip(self).await
    }
}
