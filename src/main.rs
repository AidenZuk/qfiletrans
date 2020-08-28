// mod mini_client;
// mod mini_server;
// mod protocol;
// mod file_manager;
//
// use mini_server::start_server;

//mod file_manager;

// mod file_manager;
// mod protocol;

use std::net::TcpStream;
use std::net::TcpListener;
use async_std::prelude::*;
use anyhow::Result;
use async_std::io;
use tokio;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::Arc;
use std::time::Duration;
use async_std::task;
use tokio::sync::{oneshot, mpsc};
use std::thread;
use std::ops::Deref;

use structopt::StructOpt;
use rand::prelude::*;
use std::io::{Read, Write};
use log::{info,error};
use std::fs::OpenOptions;
use std::path::PathBuf;

/// Search for a pattern in a file and display the lin that contain it.
#[derive(StructOpt)]
struct Cli {
    /// The pattern to look for
    pub mode: String,
    /// The path to the file to read
    #[structopt(short = "d", long = "dest")]
    pub dest: String,
    /// The path to the file to read
    #[structopt(parse(from_os_str))]
    pub path: std::path::PathBuf,
}
const buf_len:usize = 256 as usize*1024*1024;
fn start_server(parent_path:std::path::PathBuf) {
    let listener = TcpListener::bind("0.0.0.0:8081").unwrap();
  //  let mut incoming = listener.incoming();


    for stream in  listener.incoming() {

        let stream = stream.unwrap();
        let (reader, writer) = &mut (&stream, &stream);
        let ran_val = rand::random::<u32>();
        let file_name = format!("file_{}",ran_val);
        let mut open_option = OpenOptions::new();
        if let  Ok(mut file) = open_option.append(true).create_new(true).open(parent_path.join(&file_name)){
            let mut buffer = vec![0u8;buf_len];
            let mut cur_read_offset = 0usize;
            loop {
                match reader.read(&mut buffer[cur_read_offset..buf_len]) {
                    Ok(read_size) => {
                        if read_size == 0 {
                            info!("read finished!");
                            match file.write(&mut buffer[0..cur_read_offset]) {
                                Ok(write_size) => {
                                    if write_size != cur_read_offset {
                                        error!("error in write file size unmatched:{}/{}", read_size,cur_read_offset);

                                    }
                                    //file.sync_data();
                                },
                                Err(e)=>{
                                    error!("error in write file :{}", e.to_string());
                                }
                            }

                            break;
                        }
                        if cur_read_offset + read_size < buf_len {
                            cur_read_offset += read_size;
                        }else{
                            cur_read_offset += read_size;
                            match file.write(&mut buffer[..]){
                                Ok(write_size) => {
                                    if write_size != cur_read_offset {
                                        error!("error in write file size unmatched:{}/{}", read_size,cur_read_offset);
                                        break;
                                    }
                                    thread::sleep(std::time::Duration::from_millis(400));
                                    //file.sync_data();
                                },
                                Err(e)=>{
                                    error!("error in write file :{}", e.to_string());
                                }
                            }
                            cur_read_offset = 0;
                        }
                    },
                    Err(e) => {
                        error!("error in read:{}", e.to_string())
                    }
                }
           }

        }else{
            error!("error in write:{}",&file_name)
        }


    }
}

fn start_client(dest:String,file_name:std::path::PathBuf) {
    let mut buffer = vec![0u8;128*1024*1024];
    let mut stream = TcpStream::connect(dest).unwrap();
    let (reader, writer) = &mut (&stream, &stream);
    let mut open_option = OpenOptions::new();
    match  open_option.read(true).open(&file_name) {
        Ok(mut file) =>{
            loop {
                match file.read(&mut buffer[..]) {
                    Ok(read_size) =>{
                        if read_size == 0 {
                            info!("readfinished");
                            break;
                        }else{
                            match writer.write_all(&buffer[0..read_size]) {
                                Ok(_)=>{

                                },
                                Err(e) =>{
                                    error!("error in write stream:{}", e.to_string())
                                }
                            }
                        }
                    },
                    Err(e)=>{
                        error!("error in read file:{}", e.to_string())
                    }
                }
            }
    },
        Err(e) =>{
            error!("read file {:?} failed:{:?}",&file_name.display(),e.to_string())
        }

    }
}

fn main() {


    let args = Cli::from_args();
    if args.mode == String::from("server") {
        start_server(args.path);
    } else if args.mode == String::from("client") {
        start_client(args.dest,args.path);
    }
}
