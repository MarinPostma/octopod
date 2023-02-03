#[doc(hidden)]
pub mod sealed;

mod driver;
mod emitter;
mod resource;

use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::Context;
use driver::Driver;
use emitter::{Emitter, LogLine, TestResult};
use futures::{stream::SelectAll, Stream, StreamExt};
use resource::Resources;
use sealed::{TestDecl, TestFn};

pub use octopod_macros::test;

pub struct Octopod {
    driver: Driver,
    suites: Vec<TestSuite>,
    emitter: Emitter,
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
        let emitter = Emitter::default();

        Ok(Self {
            driver,
            suites,
            emitter,
        })
    }

    /// print all logs, even successes
    pub fn log_all(mut self) -> Self {
        self.emitter.log_all = true;
        self
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        for suite in self.suites {
            let mut resources = Resources::default();
            if let Err(e) = suite
                .run(&self.driver, &mut self.emitter, &mut resources)
                .await
            {
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
            let mut log_stream = app.logs(driver);
            let fut = f.call(app);
            //FIXME: Maybe we should fork here, and collect stdout
            let mut test_fut = tokio::spawn(fut);
            let mut logs = Vec::new();
            loop {
                tokio::select! {
                    res = &mut test_fut => {
                        let result = match res {
                            Ok(_) => TestResult::pass(name, Some(logs)),
                            Err(e) => {
                                let msg = match e.try_into_panic() {
                                    Ok(panic) => {
                                        if let Some(e) = panic.downcast_ref::<&str>() {
                                            e.to_string()
                                        } else if let Ok(e) = panic.downcast::<String>() {
                                            *e
                                        } else {
                                            "task panicked with no message".into()
                                        }
                                    }
                                    Err(e) => e.to_string(),
                                };
                                TestResult::fail(name, msg, Some(logs))
                            }
                        };
                        emitter.emit(result);
                        break;
                    }
                    Some(entry) = log_stream.next() => {
                        logs.push(entry);
                    }
                }
            }
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

    fn logs(&self, driver: &Driver) -> impl Stream<Item = LogLine> {
        let mut streams = SelectAll::new();
        for service in self.services.values() {
            streams.push(driver.logs(service));
        }

        streams
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
    name: String,
    net: Network,
    id: String,
    driver: Driver,
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
