# Octopod

Octopod is a rust testing library for testing multi-service networked application.

## Introduction

Octopod uses podman to instantiate isolated multi-service apps, and lets you run test against them, in an isolated fashion. You can define arbitrary application from container images, and write rust tests in a natural manner. Octopod takes care of instantiating your app, and providing you with the necessary information to communicate with it programatically.

## Getting started

Octopod works as a separate binary, so start by creating a new application, and add octopod as a dependency:

```bash
cargo new octopod-test
cd octopod-test
cargo add octopod
cargo add tokio --features full
cargo add hyper --features h2
```

Octopod has the concepts of `App` and `Service`. `Services` are containerised application, described using the `ServiceConfig` struct, an app in made one or more `Service`s talking to each over a network. All services within an app see each other and are adressable through their service name, thanks to podman built-in DNS.

Let create a simple `App` consising of a single service:

```rust
use octopod::{AppConfig, Octopod, ServiceConfig};

#[tokio::main]
async fn main() {
    let mut app_config = AppConfig::new("echo");
    app_config.add_service(ServiceConfig {
        name: "echo_service".into(),
        image: "simple_echo".into(),
        env: vec![],
    });

    Octopod::init(
        "unix:///var/run/docker.sock", // <- change this to the address podman is listening to
        vec![app_cofig],
    )?
    .run()
    .await?;

}
```

We can now start writing tests against this service:

```rust
#[octopod::test(app = "hello")]
async fn test(app: App) {
    // retrieve the ip address of the intanciated app
    let primary_ip = app.service("echo_service").unwrap().ip().await.unwrap();
    let resp = client::get(format!("http://{primary_ip}:8080/")).await.unwrap();
    assert_eq!(resp.status(), 200);
}
```

## Requirements
Octopod only works on linux, and requires podman 4 to be installed. Furthermore, the podman API service should be enabled so that octopod can communitate with it.
