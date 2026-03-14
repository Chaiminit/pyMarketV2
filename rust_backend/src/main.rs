mod finance;
mod trader;
mod bot;
mod protocol;
mod server;
mod utils;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::server::Server;

#[tokio::main]
async fn main() {
    println!("PyMarket V2 Backend Starting...");

    let server = Arc::new(RwLock::new(Server::new()));

    // Start the server
    let server_clone = Arc::clone(&server);
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run_server(server_clone, "127.0.0.1:8765").await {
            eprintln!("Server error: {}", e);
        }
    });

    println!("Server listening on 127.0.0.1:8765");

    // Wait for server
    if let Err(e) = server_handle.await {
        eprintln!("Server task error: {}", e);
    }
}
