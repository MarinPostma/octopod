use crate::driver::Driver;

#[derive(Default)]
struct Resources {
    resources: Vec<Box<dyn Resource>>,
}

impl Resources {
    async fn cleanup(self, driver: &Driver) {
        for resource in self.resources.iter().rev() {
            if let Err(e) = resource.free(driver).await {
                eprintln!("error freeing service: {e}");
            }
        }
    }

    fn register(&self, resource: impl Resource) {
        self.resources.push(Box::new(resource));
    }
}

#[async_trait::async_trait]
trait Resource {
    async fn free(self, driver: &Driver) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl Resource for Service {
    async fn free(self, driver: &Driver) -> anyhow::Result<()> {
        driver.destroy_service(self).await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl Resource for Network {
    async fn free(self, driver: &Driver) -> anyhow::Result<()> {
        driver.destroy_network(self).await?;

        Ok(())
    }
}
