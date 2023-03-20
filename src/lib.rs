extern crate base64;
extern crate futures;
extern crate hyper;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate tokio;
use hyper::body::{Sender, Bytes};
use hyper::server::conn::AddrStream;
use hyper::service::{service_fn, make_service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Mutex};
use std::time::{Instant};

#[derive(Debug)]
pub struct Client{
    sender: hyper::body::Sender,
    id: i32,
    first_error: Option<Instant>,
}

impl Client{
    fn send_chunk(&mut self, chunk: Bytes) -> Result<(), Bytes>{
        let result = self.sender.try_send_data(chunk);

        match (&result, self.first_error){
            (Err(_), None) => {
                self.first_error = Some(Instant::now());
            }
            (Ok(_), Some(_)) => {
                // Clear error when write succeeds
                self.first_error = None;
            }
            _ => {}
        }
        result
    }
}


type Channel = Vec<Client>;
type Channels = HashMap<String, Channel>;
#[derive(Debug)]
pub struct EventServer{
    channels: Mutex<Channels>,
    id_storage: Mutex<Vec<i32>>,
}

impl EventServer{
    pub fn summon() -> EventServer{
        EventServer{
            channels: Mutex::new(HashMap::new()),
            id_storage: Mutex::new(vec![])
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
        //finished for now
    }

    pub fn register_channel(&self, channel: &str){
        let mut channels = self.channels.lock().expect("Could not open channel lock");
        channels.insert(channel.to_owned(), Vec::new());
        println!("{:?}", channels);
    }

    fn add_client(&self,channel: &str, sender: Sender) -> bool{
        let mut channels = self.channels.lock().expect("Could not open channel lock");
        if !channels.contains_key(channel) {
            return false
        }
        let available_id = self.assign_id();
        match channels.entry(channel.to_owned()) {
            Entry::Occupied(mut e) => {
                e.get_mut().push(
                    Client{
                        sender,
                        id: available_id,
                        first_error: None
                    }
                )
            }
            Entry::Vacant(e) => {
                e.insert(Vec::new()).push(
                    Client{
                        sender,
                        id: available_id,
                        first_error: None
                    }
                )
            }
        }
        true
    }

    pub fn send_to_channel(&self, channel: &str, event: &str, message: &str){
        let mut channels = self.channels.lock().unwrap();
        let payload = format!("event: {}\ndata: {}\n\n", event, message);

        match channels.get_mut(channel) {
            Some(clients) => {
                for client in clients.iter_mut() {
                    let chunk = Bytes::from(payload.clone());
                    client.send_chunk(chunk).ok();
                }
            }
            None => {} // Currently no clients on the given channel
        };
    }

    pub fn send_to_all_channels(&self, channel: &str, event: &str, message: &str){
        let channels = self.channels.lock().expect("Could not open Channel lock");
        for channel in channels.keys() {
            self.send_to_channel(channel, event, message);
        }
    }

    fn assign_id(&self) -> i32{
        let mut lowest = 1;
        let mut id_storage = self.id_storage.lock().expect("Could not open ID lock");
        while id_storage.contains(&lowest){
            lowest += 1;
        }
        id_storage.push(lowest);
        lowest
    }

    pub async fn create_stream(&self, req: Request<Body>) -> Response<Body>{
        // Extract channel from uri path (last segment)
        let channel = req.uri().path().rsplit("/").next().expect("Could not get Channel Path");
        // println!("Connection accepted");
        let (sender, body) = Body::channel();
        let result = self.add_client(channel, sender);
        if !result {
            return Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::empty())
                    .expect("Could not create response");
        }
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