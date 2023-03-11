extern crate base64;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
use hyper::body::Sender;
use hyper::client::service;
use hyper::server::conn::AddrStream;
use hyper::service::{service_fn, make_service_fn};
use hyper::{Body, Request, Response, Server};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug)]
pub struct Client{
    sender: hyper::body::Sender,
    id: i32,
    first_error: Option<Instant>,
}

impl Client{
    fn send_chunk(&mut self){}
}


type Channel = Vec<Client>;
type Channels = HashMap<String, Channel>;

pub struct EventServer{
    channels: Mutex<Channels>,
    next_id: i32,
}

impl EventServer{
    pub fn summon() -> EventServer{
        EventServer{
            channels: Mutex::new(HashMap::new()),
            next_id: 0
        }
    }

    pub async fn spawn(&'static self){
        let addr = SocketAddr::from(([0, 0, 0, 0], 22717));
        
        let sse_handler = make_service_fn(|_socket: &AddrStream| {
            async move {
                Ok::<_,Infallible>(service_fn(move |req: Request<Body>| async move {
                    Ok::<_,Infallible>(self.create_stream(req).await)
                }))
            }
        });

        let server = Server::bind(&addr)
        .serve(sse_handler);

        // Finally, spawn `server` onto an Executor...
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }

    fn add_client(&self,channel: &str, sender: Sender){
        match self.channels.lock().expect("Could not open channel lock").entry(channel.to_owned()) {
            Entry::Occupied(mut e) => {
                e.get_mut().push(
                    Client{
                        sender,
                        id: 514,
                        first_error: None
                    }
                )
            }
            Entry::Vacant(e) => {
                e.insert(Vec::new()).push(
                    Client{
                        sender,
                        id: 514,
                        first_error: None
                    }
                )
            }
        }
    }

    pub async fn create_stream(&self, req: Request<Body>) -> Response<Body>{
        let channel = "tokens";
        println!("Connection accepted");
        let (sender, body) = Body::channel();
        self.add_client(channel, sender);
        println!("{:?}", self.channels);

        Response::builder()
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no")
            .header("Content-Type", "text/event-stream")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .expect("Could not create response")
    }
}