use std::collections::HashMap;
use crate::protocol::{ ProtocolParser,ProtocolMsgs,ProtoProcIntf,create_file_req,create_file_resp,create_chunk_req,create_chunk_resp};
use std::fs::{OpenOptions,File};
use anyhow::{*,Result};
use std::path::Path;
use rayon::Scope;
use log::*;
use std::net::TcpStream;
use async_std::{
    prelude::*, // 1
    task, // 2
    net::{TcpListener, ToSocketAddrs}, // 3
};
use std::io::{*};
use std::sync::{Arc,Mutex};
use std::sync::mpsc::{Receiver,SyncSender,sync_channel,RecvError};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

pub struct FileServer<'a>{
    file_handle:HashMap<u32,File>,
    config_path:String,
    receiver:Receiver<ProtocolMsgs<'a>>,
    write:Box<dyn Write>,
}
impl <'a>  FileServer<'a>{
    fn new(config_path:String,receiver:Receiver<ProtocolMsgs<'a>>,writer:Box<dyn Write >)->Self{
        FileServer {
            file_handle:HashMap::new(),
            config_path:config_path.clone(),
            receiver,
            write:writer
        }
    }
}
impl<'a> ProtoProcIntf<'a> for FileServer<'a> {
    fn recv(&self) -> Result<ProtocolMsgs<'a>, RecvError>{
        self.receiver.recv()
    }
    fn  on_write_file_req (&mut self, handle_id:u32, file_name:String,file_len:u64)->bool{
        let result = if let Some(file) = &self.file_handle.get(&handle_id) {
            error!("last file exists!");
            true
        } else {
            let mut  options = OpenOptions::new();

            match options.write(true).create(true).open(Path::new(&file_name)) {
                Ok(file) =>{
                    if let Ok(_) = file.set_len(file_len) {
                        self.file_handle.insert(handle_id,file);

                        false
                    }else{
                        error!("set file len failed! {}",&file_name);
                        true
                    }
                },
                Err(e) =>{
                    error!("open file {} failed! reason:{}",&file_name,e.to_string());
                    true
                }
            }
        };
        let resp_vec = if result == false{
            create_file_resp(handle_id,0)
        }else{
            create_file_resp(handle_id,1)
        };
        self.write.as_mut().write(&resp_vec.unwrap()[..]);
        result
    }

    fn on_write_file_resp  (&mut self, handle_id:u32, status :u8)->bool {
        false
    }
    fn on_write_chunk_req(&mut self, handle_id:u32, chunk_id:u32, chunk_start:u64, chunk_len:u64,chunk_data:Arc<Mutex<&[u8]>>) ->bool {
        let result:u8 = if let Some(file) = self.file_handle.get_mut(&handle_id) {
            if let Ok(_) = file.seek(SeekFrom::Start(chunk_start)) {
                if let Ok(_) = file.write(chunk_data.lock().unwrap().deref()){
                        0
                }else{
                    1
                }
            }else{
                2
            }

        }else{
            3
        };

        if result == 0 {
            if chunk_start + chunk_data.lock().unwrap().deref().len() as u64 >= chunk_len{
                let resp_vec = create_chunk_resp(handle_id,chunk_id,result);
                self.write.as_mut().write(&resp_vec.unwrap()[..]);
            };
            false
        }else{
            let resp_vec = create_chunk_resp(handle_id,chunk_id,result);
            self.write.as_mut().write(&resp_vec.unwrap()[..]);
            true
        }
    }
    fn on_write_chunk_resp(&mut self, handle_id:u32,chunk_id:u32, status:u8) ->bool{
        false
    }
}

pub enum UploadState{
    US_IDLE,
    US_WRITING_H,
    US_WRITING_CHUNK,
    US_WRITING_END,
}
pub struct FileClient<'a>{
    file_handle:Option<File>,
    buf_size:u64,
    handle_id:u32,
    time_out:Duration,
    config_path:String,
    file_name:String,
    cur_state:UPLOAD_STAT
}
pub struct UpEvtStart {
    file_name:String,
}
pub struct UpEvtHeaderResp {
    handle_id:u32,
    status:u8
}
pub struct UpEvtChunkResp {
    handle_id:u32,
    chunk_id:u32,
    start:u64,
    len:u64,
    status:u8
}
pub struct UpEvtTimeout ();
pub struct UpEvtOtherError{
    reason:u8
}

pub enum UpEvent{
    EvtStart(UpEvtStart),
    EvtHeadResp(UpEvtHeaderResp),
    EvtChunkResp(UpEvtChunkResp),
    EvtTimeOut,
    EvtOther(UpEvtOtherError)
}
impl<'a> FileClient<'a> {
    pub fn new(config_path:String,file_name:String,receiver:Receiver<ProtocolMsgs<'a>>,writer:Box<dyn Write>,buf_len:u64,time_out:Duration)->Self{
        let mut  options = OpenOptions::new();

        let file = match options.read(true).open(Path::new(&file_name)) {
            Ok(file) =>{
                Some(file)
            },
            Err(e) =>{
                error!("error in open file {}, reason: {}",&file_name,e.to_string());
                None
            }
        };
        FileClient{
            file_handle:file,
            buf_size:buf_len,
            handle_id:0,
            time_out,
            config_path,
            file_name,
            cur_state:UploadState::US_IDLE,
        }
    }

    pub async fn process(&mut self, event: UpEvent, mut writer:impl Write) ->tokio::Result<bool>{

        match event {
            UpEvent::EvtStart(start_req) =>{
                let mut  options = OpenOptions::new();

                match options.read(true).open(Path::new(&(&self.config_path + &start_req.file_name))) {
                    Ok(file) =>{
                        self.file_handle = Some(file);
                        let len = file.metadata().unwrap().len();
                        if let Ok(req_vec) = create_file_req(start_req.file_name.clone(),len) {
                            writer.write(&req_vec[..]).await;
                            self.cur_state = UploadState::US_WRITING_H;
                            Ok(false)
                        }else{
                            Err(anyhow!("open file {}"))
                        }
                    },
                    Err(e) =>{
                        error!("error in open file {}, reason: {}",&file_name,e.to_string());
                        Err(e)
                    }
                }
            },
            UpEvent::EvtHeadResp(h_resp)=>{
                if self.cur_state == UploadState::US_WRITING_H {
                    if h_resp.status == 0 {
                        self.handle_id = 0;
                    }
                }
            },
            UpEvent::EvtChunkResp(c_resp)=>{

            },
            UpEvent::EvtTimeOut =>{

            },
            UpEvent::EvtOther(other_err)=>{

            }

        }
    }
}
impl<'a> ProtoProcIntf<'a> for FileClient<'a> {
    fn recv(&self) -> Result<ProtocolMsgs<'a>, RecvError>{
        self.receiver.recv_timeout(self.time_out).map_err(|e| { RecvError })
    }
    fn  on_write_file_req (&mut self, handle_id:u32, file_name:String,file_len:u64)->bool{
       false
    }

    fn on_write_file_resp  (&mut self, handle_id:u32, status :u8)->bool {
        false
    }
    fn on_write_chunk_req(&mut self, handle_id:u32, chunk_id:u32, chunk_start:u64, chunk_len:u64,chunk_data:Arc<Mutex<&[u8]>>) ->bool {
       false
    }
    fn on_write_chunk_resp(&mut self, handle_id:u32,chunk_id:u32, status:u8) ->bool{
        false
    }
}

pub fn start( s: &Scope,full_path:String,remote_ip:String,remote_port:u16,buf_size:usize,time_out:Duration){
    //open file as reader

    // create send,receive pair
    match File::open(full_path,) {
        Ok(file) =>{
            //open tcp client
            let addr_info = format!("{}:{}",remote_ip,remote_port);


            let mut stream = TcpStream::connect(addr_info)
                .expect("Couldn't connect to the server...");
            stream.set_write_timeout(None).expect("set_write_timeout call failed");
            let (tcp_read,tcp_write) = (Arc::new(Mutex::new(&mut stream)),Arc::new(Mutex::new(Some(Box::new( stream)))));
            let (sender,receiver) = sync_channel(0);
            //create protocol parser
            let mut protocol =ProtocolParser::new(buf_size);
            let mut file_uploader =  FileClient::new(String::from("/home/zhu/"), String::from("test.txt"), receiver, tcp_write.clone(), buf_size as u64, time_out);
            protocol.build(sender.clone());

            s.spawn(move |_s|{
                //build protocol parser with self

                //start main routine
                protocol.run(tcp_read,tcp_write.clone())
            });

            s.spawn(move |_s|{
               file_uploader.do_upload();
            })


            //start state process routine
        },
        Err(e) =>{
            error!("read file {} failed, caused by:{}",full_path,e.into());
        }
    }

}
// fn start_protocol (buf_size:usize,reader:impl Read + 'static,writer:impl Write + 'static){
//     let file_manager =  FileManager::new(String::from("/home/zhu/"));
//     let mut protocol =ProtocolParser::new(buf_size);
//     protocol.build(Arc::new(Mutex::new(file_manager)));
//     protocol.run(reader,writer)
//
// }
#[cfg(test)]
mod Test{

}