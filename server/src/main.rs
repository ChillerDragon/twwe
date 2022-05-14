extern crate pretty_env_logger;

use std::{
    collections::HashMap,
    env,
    error::Error,
    io::Error as IoError,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tungstenite::protocol::Message;

use serde::{Deserialize, Serialize};

use twmap::{Layer, TwMap};

type Tx = UnboundedSender<Message>;

struct State {
    peers: HashMap<SocketAddr, Tx>,
    maps: HashMap<String, TwMap>,
}

impl State {
    fn new() -> State {
        State {
            peers: HashMap::new(),
            maps: HashMap::new(),
        }
    }
}

type Res<T> = Result<T, Box<dyn Error>>;

type SharedState = Arc<Mutex<State>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Change {
    map_name: String,
    group: u32,
    layer: u32,
    x: u32,
    y: u32,
    id: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct Users {
    count: u32,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "content", rename_all = "lowercase")]
enum JsonRequest {
    Change(Change),
    Map(String),
    Save(String),
}

#[derive(Serialize)]
#[serde(tag = "type", content = "content", rename_all = "lowercase")]
enum JsonResponse {
    Change(Change),
    // Map(TwMap), // TODO: we need to find a way to send the map to new clients without desync
    Users(Users),
}

fn send_map(state: SharedState, addr: SocketAddr, map_name: String) -> Res<()> {
    // TODO: this is blocking for the server
    let mut state = state.lock().map_err(|_| "could not acquire lock")?;
    let map = state.maps.get_mut(&map_name).ok_or("map not found")?;

    let mut buf = Vec::new();
    map.save(&mut buf)?;

    let peer = state.peers.get(&addr).ok_or("failed to get peer")?;
    peer.unbounded_send(Message::Binary(buf))?;

    Ok(())
}

fn save_map(state: SharedState, map_name: &str) -> Res<()> {
    // clone the map to release the lock as soon as possible
    let mut map = {
        let mut state = state.lock().map_err(|_| "could not acquire lock")?;
        let map = state.maps.get_mut(map_name).ok_or("map not found")?;
        map.clone()
    };
    let path = format!("maps/{}.map", map_name);
    map.save_file(path)?;
    map.load()?;
    log::info!("saved {}", map_name);
    Ok(())
}

fn set_tile(state: SharedState, change: Change) -> Res<()> {
    let mut state = state.lock().map_err(|_| "could not acquire lock")?;
    let map = state
        .maps
        .get_mut(&change.map_name)
        .ok_or("map not found")?;
    let layer = map
        .groups
        .iter_mut()
        .find_map(|g| {
            g.layers.iter_mut().find_map(|l| match l {
                Layer::Game(gl) => Some(gl),
                _ => None,
            })
        })
        .ok_or("no game layer")?;
    let tiles = layer.tiles.unwrap_mut(); // map must be loaded
    let mut tile = tiles
        .get_mut((change.y as usize, change.x as usize))
        .ok_or("tile change outside layer")?;
    tile.id = change.id;
    log::debug!("changed pixel {:?}", change);

    // broadcast message
    let str = serde_json::to_string(&JsonResponse::Change(change)).unwrap();
    let msg = Message::Text(str);
    for (_addr, peer) in state.peers.iter() {
        peer.unbounded_send(msg.clone()).unwrap();
    }

    Ok(())
}

fn broadcast_users(state: SharedState) {
    let state = state.lock().unwrap();
    let n = state.peers.len();
    let resp = Users { count: n as u32 };
    let str = serde_json::to_string(&JsonResponse::Users(resp)).unwrap();
    let msg = Message::Text(str);
    for (_addr, peer) in state.peers.iter() {
        peer.unbounded_send(msg.clone()).unwrap();
    }
}

fn handle_request(state: SharedState, addr: SocketAddr, req: JsonRequest) -> Res<()> {
    match req {
        JsonRequest::Change(change) => set_tile(state, change),
        JsonRequest::Map(map_name) => send_map(state, addr, map_name),
        JsonRequest::Save(ref map_name) => save_map(state, map_name),
    }
}

async fn handle_connection(state: SharedState, raw_stream: TcpStream, addr: SocketAddr) {
    println!("Incoming TCP connection from: {}", addr);

    // accept
    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    log::info!("WebSocket connection established: {}", addr);

    // insert peer in peers
    let (tx, rx) = unbounded();
    state.lock().unwrap().peers.insert(addr, tx);

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        let text = msg.to_text().unwrap();
        log::debug!("message received: {}", text);
        let req = serde_json::from_str(text);
        match req {
            Ok(req) => handle_request(state.clone(), addr, req).unwrap_or_else(|e| {
                log::error!("error occured while handling request: {}", e);
            }),
            Err(e) => {
                log::error!("failed to parse message: {}", e);
            }
        };
        future::ok(())
    });

    broadcast_users(state.clone()); // login
    {
        let receive_from_others = rx.map(Ok).forward(outgoing);
        pin_mut!(broadcast_incoming, receive_from_others);
        future::select(broadcast_incoming, receive_from_others).await;

        println!("{} disconnected", &addr);
        state.lock().unwrap().peers.remove(&addr);
    }
    broadcast_users(state.clone()); // logout
}

fn add_map(state: &mut State, name: &str) {
    let path = format!("maps/{}.map", name);
    let mut map = TwMap::parse_file(&path).expect("failed to parse map");

    // use std::time::Instant;
    // let now = Instant::now();
    // map.save_file(&format!("{}.map", path));
    // let elapsed = now.elapsed();
    // println!("Elapsed: {:.2?}", elapsed);

    map.load().expect("failed to load map");
    state.maps.insert(name.to_string(), map);
    log::info!("loaded map {}", name);
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    pretty_env_logger::init();
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:16800".to_string());

    let mut state = State::new();
    add_map(&mut state, "Sunny Side Up");
    let state = SharedState::new(Mutex::new(state));

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(state.clone(), stream, addr));
    }

    Ok(())
}
