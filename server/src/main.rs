use std::sync::Arc;

use clap::Parser;
use cli::Cli;

use glob::glob;
use router::Router;
use server::Server;

mod base64;
mod checks;
mod cli;
mod error;
mod map_cfg;
mod protocol;
mod room;
mod router;
mod server;
mod twmap_map_checks;
mod twmap_map_edit;
mod util;

use room::Room;

fn create_server(cli: &Cli) -> Server {
    let server = Server::new(cli);
    {
        let mut server_rooms = server.rooms();
        let rooms = glob("maps/*/map.map")
            .expect("no map found in maps directory")
            .filter_map(|e| e.ok())
            .map(|e| {
                let dir = e.parent().unwrap().to_owned(); // map must be in a sub-directory
                let room = Room::new(dir).expect("failed to load one of the map dirs");

                // ensure the room has all the required resources
                if !room.cfg_path().exists() {
                    room.save_config().expect("failed to create config file");
                }
                if !room.automapper_path().exists() {
                    std::fs::create_dir(room.automapper_path())
                        .expect("failed to create automapper dir");
                }

                Arc::new(room)
            });
        for r in rooms {
            server_rooms.insert(r.name().to_owned(), r);
        }
    }
    log::info!("found {} maps.", server.rooms().len());
    server
}

#[tokio::main]
async fn run_server(args: Cli) {
    let server = Arc::new(create_server(&args));

    let router = Router::new(server, &args);
    router.run(&args).await;
}

fn main() {
    pretty_env_logger::init_timed();

    let args = Cli::parse();

    run_server(args);
}
