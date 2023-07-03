//! Minimalist Server-Sent Event Library\
//! This library is intended to be used with axum as the examples will show.\
//! This crate will not be maintained and the source code can be found on GitHub: <https://github.com/Tibuarka/rust_sse>
// Test
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


/// A struct representing a connected client
/// # Arguments
/// * `sender` - A Sender struct by hyper
/// * `id` - A unique id for a connected client
/// * `first_error` - When a client cannot recieve an event the time will be saved for maintenance
#[derive(Debug)]
pub struct Client{
    sender: hyper::body::Sender,
    id: i32,
    first_error: Option<Instant>,
}

impl Client{
    /// Sends an event as a chunk of bytes to a client
    /// # Arguments
    /// * `chunk` - the event formatted as a chunk of bytes
    /// If an error occurs the timestamp will be saved to the client for the maintenance function of the EventServer
    fn send_chunk(&mut self, chunk: Bytes){
        let result = self.sender.try_send_data(chunk);

        match (result, self.first_error){
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
/// Represents a Vector of Client structs
type Channel = Vec<Client>;
/// Represents a HashMap with channels and their respective Client vector
type Channels = HashMap<String, Channel>;
/// A hyper HTTP server for handling Server-Sent Events
/// # Arguments
/// * `channels` - a HashMap with channels which contain a vector with Client structs
/// * `id_storage` - a vector with IDs for Clients
/// # Notes
/// The server automatically gives out the lowest available ID to a Client and removes it from the vector if a Client is removed 
#[derive(Debug)]
pub struct EventServer{
    channels: Mutex<Channels>,
    id_storage: Mutex<Vec<i32>>,
}

impl EventServer{
    /// Returns an EventServer
    pub fn summon() -> EventServer{
        EventServer{
            channels: Mutex::new(HashMap::new()),
            id_storage: Mutex::new(Vec::new())
        }
    }
    /// Starts the server as a seperate http server
    /// # Arguments
    /// * `port` - the specified port the server runs on
    pub async fn spawn(&'static self, port: u16){
        // Binds the specified port to a socket address
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        let sse_handler = make_service_fn(|_socket: &AddrStream| {
            async move {
                Ok::<_,Infallible>(service_fn(move |req: Request<Body>| async move {
                    Ok::<_,Infallible>(self.create_stream(req))
                }))
            }
        });
        let server = Server::bind(&addr).serve(sse_handler);
        if let Err(e) = server.await {
            eprintln!("server error: {}", e);
        }
    }

    /// Adds a channel into the event server channel storage
    /// # Arguments
    /// * `channel` - a specific singular eventsource channel
    pub fn register_channel(&self, channel: &str){
        let channels = self.channels.lock();
        match channels {
            Ok(mut channels) => {
                channels.insert(channel.to_owned(), Vec::new());
            }
            Err(_) => { eprintln!("Could not lock channels"); }
        }
    }

    /// Adds a client to a channel
    /// # Arguments
    // * `channel` - the channel of the EventSource
    // * `sender` - the Sender struct from hyper
    fn add_client(&self, channel: &str, sender: Sender) -> Result<(), &str>{
        let channels = self.channels.lock();
        match channels {
            Ok(mut channels) => {
                if !channels.contains_key(channel) {
                    return Err("Channel nonexistent")
                }
                match self.assign_id() {
                    Ok(client_id) => {
                        match channels.entry(channel.to_owned()) {
                            Entry::Occupied(mut e) => {
                                e.get_mut().push(
                                    Client{
                                        sender,
                                        id: client_id,
                                        first_error: None
                                    }
                                )
                            }
                            Entry::Vacant(e) => {
                                e.insert(Vec::new()).push(
                                    Client{
                                        sender,
                                        id: client_id,
                                        first_error: None
                                    }
                                )
                            }
                        }
                        Ok(())
                    },
                    Err(e) => return Err(e),
                }
            }
            Err(_) => { return Err("Could not lock channels") }
        }
    }

    /// Send an event to a singular channel
    /// # Arguments
    /// * `channel` - a specific singular eventsource channel
    pub fn send_to_channel(&self, channel: &str, event: &str, message: &str){
        let channels = self.channels.lock();
        match channels {
            Ok(mut channels) => {
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
            }
            Err(_) => { eprintln!("Could not lock channels"); }
        }
    }

    /// Send an event to all registered channels
    /// # Arguments
    /// * `event` - the event type
    /// * `message` - the data for the event
    pub fn send_to_all_channels(&self, event: &str, message: &str){
        let channels = self.get_channels();
        match channels {
            Ok(channels) => {
                for channel in channels {
                    self.send_to_channel(&channel, event, message);
                }
            }
            Err(_) => { eprintln!("Could not get a list of channels"); }
        }
    }

    pub fn get_channels(&self) -> Result<Vec<String>, &str> {
        let channel_mutex = self.channels.lock();
        match channel_mutex {
            Ok(channel_mutex) => {
                let channel_vector: Vec<String> = channel_mutex.keys().map(|channel| channel.to_owned()).collect();
                drop(channel_mutex);
                Ok(channel_vector)
            },
            Err(_) => { return Err("Could not lock channels") }
        }
    }

    fn remove_stale_clients(&self){
        let channel_mutex = self.channels.lock();
        match channel_mutex {
            Ok(mut channel_mutex) => {
                for (_, clients) in channel_mutex.iter_mut() {
                    clients.retain(|client| {
                        if let Some(first_error) = client.first_error {
                            if first_error.elapsed() > Duration::from_secs(5) {
                                let id_storage = self.id_storage.lock();
                                match id_storage {
                                    Ok(mut id_storage) => {
                                        id_storage.retain(|&stored_id| stored_id != client.id);
                                        return false;
                                    },
                                    Err(_) => eprintln!("Could not lock ID storage"),
                                }
                            }
                        }
                        true
                    })
                }
            },
            Err(_) => { eprintln!("Could not lock channels"); },
        }
    }
    /// Regular maintenance of Clients based on a specified interval
    /// # Arguments
    /// * `delay` - specified interval in seconds
    pub async fn maintenance(&self, delay: u64){
        let mut tick = interval(Duration::from_secs(delay));
        loop {
            tick.tick().await;
            self.remove_stale_clients();
            self.send_to_all_channels("heartbeat", "");
        }
    }

    fn assign_id(&self) -> Result<i32, &str>{
        let mut lowest = 1;
        let id_storage = self.id_storage.lock();
        match id_storage {
            Ok(mut id_storage) => {
                while id_storage.contains(&lowest){
                    lowest += 1;
                }
                id_storage.push(lowest);
                Ok(lowest)
            },
            Err(_) => {
                return Err("Could not lock ID storage")
            },
        }
    }

    pub fn create_stream(&self, req: Request<Body>) -> Response<Body>{
        let channel = req.uri().path().split("/").last().expect("Could not get Channel Path");
        let (sender, body) = Body::channel();
        let result = self.add_client(channel, sender);
        match result {
            Ok(_) => {},
            Err(err) => {
                eprintln!("Create_Stream threw an Error: {:?}", err);
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