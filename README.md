# SSE
This is a library to implement server-sent events in a web application project. The library can be used to start a seperate dedicated http server which handels SSEs or can be used with your custom handlers with the axum web framework (I did not get it to work with other frameworks).

## Examples
### Standalone HTTP server
```rust
TODO
```
### Integrated into axum with custom route handler
```rust
#[tokio::main]
async fn main() {
    // Let the maintenance run seperately
    task::spawn(async {
            SSE.maintenance(1).await;
        });

    // Moves the request to the SSE handler
    let app = Router::new().route("/event/:event",get( move |req| event_handler(req) ));

    // Start the server on port 8082
    let addr = SocketAddr::from(([127, 0, 0, 1], 8082));
    axum::Server::bind(&addr).serve(app.into_make_service()).await.expect("Failed starting the server");
}

async fn event_handler(req: Request<Body>) -> Response<Body> {
    SSE.create_stream(req)
}
```

## WARNING
- This is my _first_ rust project so this code is by no means stable and/or good! Feel free to modify it to suit your needs
- I will not accept pull requests because the amount of knowledge about rust that I have is not enough to correctly decide over your proposed changes
- There is a high chance that I will not maintain this library so use with care
