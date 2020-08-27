use std::{
    prelude::*, // 1
    task, // 2
    net::{TcpListener, ToSocketAddrs}, // 3
};
use std::io::{*};

pub fn start_server(addr:&str,port:u16)->std::io::Result<()> {
    let addr_info = format!("{}:{}",addr,port);
    let listener = TcpListener::bind("127.0.0.1:8080");
    if let Ok(listener) = listener {
        let mut incoming = listener.incoming();

        while let Some(stream) = incoming.next() {
            let stream = stream?;
            let (reader, writer) = &mut (&stream, &stream);
            //
        }
        Ok(())
    }else{
        Err(Error::new(std::io::ErrorKind::AddrNotAvailable, "failed to fill whole buffer"))
    }



}
