use hyper::body::{Sender, Bytes};
use hyper::server::conn::AddrStream;
use hyper::service::{service_fn, make_service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use tokio::time::interval;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Instant, Duration};

#[derive(Debug)]
pub struct Client{
    sender: hyper::body::Sender,
    id: i32,
    first_error: Option<Instant>,
}

impl Client{
    fn send_chunk(&mut self, chunk: Bytes){
        let result = self.sender.try_send_data(chunk);

        match (&result, self.first_error){
            (Err(_), None) => {
                self.first_error = Some(Instant::now());
            }
            (Ok(_), Some(_)) => {
                self.first_error = None;
            }
            _ => {}
        }
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
            id_storage: Mutex::new(Vec::new())
        }
    }

    pub async fn spawn(&'static self, port: u16){
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        
        let sse_handler = make_service_fn(|_socket: &AddrStream| {
            async move {
                Ok::<_,Infallible>(service_fn(move |req: Request<Body>| async move {
                    Ok::<_,Infallible>(self.create_stream(req))
                }))
            }
        });

        let server = Server::bind(&addr)
        .serve(sse_handler);

        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }

    pub fn register_channel(&self, channel: &str){
        let mut channels = self.channels.lock().expect("Could not open channel lock");
        channels.insert(channel.to_owned(), Vec::new());
    }

    fn add_client(&self,channel: &str, sender: Sender) -> Result<bool, &str>{
        let mut channels = self.channels.lock().expect("Could not open channel lock");
        if !channels.contains_key(channel) {
            return Err("Channel nonexistent")
        }
        match channels.entry(channel.to_owned()) {
            Entry::Occupied(mut e) => {
                e.get_mut().push(
                    Client{
                        sender,
                        id: self.assign_id(),
                        first_error: None
                    }
                )
            }
            Entry::Vacant(e) => {
                e.insert(Vec::new()).push(
                    Client{
                        sender,
                        id: self.assign_id(),
                        first_error: None
                    }
                )
            }
        }
        Ok(true)
    }

    pub fn send_to_channel(&self, channel: &str, event: &str, message: &str){
        let mut channels = self.channels.lock().unwrap();
        let payload = format!("event: {}\ndata: {}\n\n", event, message);

        match channels.get_mut(channel) {
            Some(clients) => {
                for client in clients.iter_mut() {
                    let chunk = Bytes::from(payload.clone());
                    client.send_chunk(chunk);
                }
            }
            None => {}
        };
        drop(channels);
    }

    pub fn send_to_all_channels(&self, event: &str, message: &str){
        let channels = self.get_channels();
        for channel in channels {
            self.send_to_channel(&channel, event, message);
        }
    }

    fn get_channels(&self) -> Vec<String> {
        let channel_mutex = self.channels.lock().unwrap();
        let channel_vector: Vec<String> = channel_mutex.keys().map(|ch| ch.to_owned()).collect();
        drop(channel_mutex);
        channel_vector
    }

    fn remove_stale_clients(&self){
        let mut channel_mutex = self.channels.lock().unwrap();
        for (_, clients) in channel_mutex.iter_mut() {
            clients.retain(|client| {
                if let Some(first_error) = client.first_error {
                    if first_error.elapsed() > Duration::from_secs(5) {
                        let mut id_storage = self.id_storage.lock().expect("Could not open ID lock");
                        id_storage.retain(|&stored_id| stored_id != client.id);
                        return false;
                    }
                }
                true
            })
        }
        drop(channel_mutex);
    }

    pub async fn maintenance(&self, t: u64){
        let mut tick = interval(Duration::from_secs(t));
        loop {
            tick.tick().await;
            self.remove_stale_clients();
            self.send_to_all_channels("heartbeat", "");
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

    pub fn create_stream(&self, req: Request<Body>) -> Response<Body>{
        let channel = req.uri().path().rsplit("/").next().expect("Could not get Channel Path");
        let (sender, body) = Body::channel();
        let result = self.add_client(channel, sender);
        match result {
            Ok(_) => {},
            Err(err) => {
                dbg!("Create Stream threw an Error: {:?}", err);
                return Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::empty())
                        .expect("Could not create response");
            }
        }

        Response::builder()
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no")
            .header("Content-Type", "text/event-stream")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .expect("Could not create response")
    }
}