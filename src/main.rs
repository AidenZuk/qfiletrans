mod mini_client;
mod mini_server;
mod protocol;
mod file_manager;

use mini_server::start_server;
fn main() {
    println!("Hello, world!");
    //create server
    start_server("127.0.0.1",6880);
}
