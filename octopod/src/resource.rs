use crate::{driver::Driver, Network, Service};

#[derive(Default)]
pub(crate) struct Resources {
    resources: Vec<Box<dyn Resource>>,
}

impl Resources {
    pub async fn cleanup(self, driver: &Driver) {
        for resource in self.resources.into_iter().rev() {
            if let Err(e) = resource.free(driver).await {
                eprintln!("error freeing service: {e}");
            }
        }
    }

    pub fn register(&mut self, resource: impl Resource + 'static) {
        self.resources.push(Box::new(resource));
    }
}

#[async_trait::async_trait]
pub(crate) trait Resource {
    async fn free(&self, driver: &Driver) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl Resource for Service {
    async fn free(&self, driver: &Driver) -> anyhow::Result<()> {
        driver.destroy_service(self).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Resource for Network {
    async fn free(&self, driver: &Driver) -> anyhow::Result<()> {
        driver.destroy_network(self).await?;

        Ok(())
    }
}
