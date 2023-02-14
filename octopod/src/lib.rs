#[doc(hidden)]
pub mod sealed;

mod driver;
mod emitter;
mod resource;
mod service;

use std::collections::HashMap;

use anyhow::Context;
use driver::Driver;
use emitter::{Emitter, LogLine, TestResult};
use futures::{stream::SelectAll, Stream, StreamExt};
use resource::Resources;
use sealed::{TestDecl, TestFn};

pub use octopod_macros::test;
pub use service::{Service, ServiceConfig};

pub struct Octopod {
    driver: Driver,
    suites: Vec<TestSuite>,
    log_all: bool,
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
                ignore: decl.ignore,
            };

            suites
                .get_mut(decl.app)
                .with_context(|| format!("unknown app `{}` in test `{}`", decl.app, decl.name))?
                .tests
                .push(test);
        }

        let suites = suites.into_values().collect();
        let driver = Driver::new(podman_addr)?;

        Ok(Self {
            driver,
            suites,
            log_all: false,
        })
    }

    /// print all logs, even successes
    pub fn log_all(mut self) -> Self {
        self.log_all = true;
        self
    }

    pub async fn run(self) -> anyhow::Result<bool> {
        let mut success = true;
        for suite in self.suites {
            let mut resources = Resources::default();
            match suite.run(&self.driver, &mut resources, self.log_all).await {
                Err(e) => {
                    eprintln!("error running test suite: {e}");
                }
                Ok(s) => success &= s,
            }

            resources.cleanup(&self.driver).await;
        }

        Ok(success)
    }
}

struct Test {
    f: &'static dyn TestFn,
    name: String,
    ignore: bool,
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

    /// Returns whether all the tests were successful
    async fn run(
        self,
        driver: &Driver,
        resources: &mut Resources,
        log_all: bool,
    ) -> anyhow::Result<bool> {
        let mut success = true;
        let mut emitter = Emitter::new(log_all);
        println!("running {} tests on {}:", self.tests.len(), self.app.name);
        for Test { name, f, ignore } in &self.tests {
            if *ignore {
                emitter.emit(TestResult::ignore(name));
                continue;
            }

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
                                // at least one test failed
                                success = false;
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

        Ok(success)
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
