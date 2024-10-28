mod plex;
mod proxy;

use std::env;

use proxy::make_proxy;
use tokio::net::TcpListener;

fn env_var(var: &str) -> String {
    let val = env::var(var).unwrap_or_else(|_| panic!("{} is not set", var));
    log::info!("{}: {}", var, val);
    val
}

#[tokio::main]
async fn main() {

    env_logger::init();
    
    let plex_url = env_var("PLEX_URL");
    let plex_library_path = env_var("PLEX_LIBRARY_PATH");
    let rclone_url = env_var("RCLONE_URL");
    let port = env_var("PORT");

    let proxy = make_proxy(plex_url, plex_library_path, rclone_url);

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, proxy).await.unwrap();
}
