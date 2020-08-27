use std::io::{ Write,Read};
use crate::protocol::GL_CHUNK_STATE::{WAIT_CMD, WAIT_STATUS_CODE, IN_CHUNK_LEN, WAIT_END};
use std::io::Cursor;
use byteorder::{BigEndian,ReadBytesExt,WriteBytesExt};

use anyhow::{*,Result};
use crate::protocol::P_TAG::{TAG_START, TAG_END};
use rand::prelude::*;
use std::alloc::handle_alloc_error;
use num;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use std::fs::{File,OpenOptions};
use std::path::{PathBuf, Path};
use std::collections::HashMap;
use std::sync::{Arc,Mutex};
use std::ops::DerefMut;
use std::sync::mpsc::{SyncSender,Receiver,sync_channel};
pub struct WriteFileReq{
    pub handle:u32,
    pub file_name:String,
    pub total_len:u64
}
pub struct WriteFileResp{
    pub handle:u32,
    pub status:u8
}
pub struct WriteChunkReq<'a>{
    pub handle:u32,
    pub chunkid:u32,
    pub start:u64,
    pub len:u64,
    pub chunk:Arc<Mutex<&'a [u8]>>
}
pub struct WriteChunkResp{
    pub handle:u32,
    pub chunkid:u32,
    pub status:u8
}

pub enum ProtocolMsgs<'a> {
    FileReq(WriteFileReq),
    FileResp(WriteFileResp),
    ChunkReq(WriteChunkReq<'a>),
    ChunkResp(WriteChunkResp)
}


#[derive(Clone,Eq, PartialEq)]
pub enum P_TAG{
    TAG_START = 0xBE,
    TAG_END  = 0x4C,
}
#[derive(Clone,Eq, PartialEq,FromPrimitive,Copy)]
pub enum CMD{
    UNKNOWN = 0x00,
    WRITE_FILE_REQ=0x01,
    WRITE_CHUNK_REQ=0x02,

    WRITE_FILE_RESP = 0x11,
    WRITE_CHUNK_RESP = 0x12
}
#[derive(Clone,Eq, PartialEq)]
pub enum GL_STATE{
    IDLE,
    IN_BYTE_MODE,
    IN_BUFFER_MODE,
}
#[derive(Clone,Eq, PartialEq,FromPrimitive)]
pub enum GL_CHUNK_STATE {
    WAIT_START,
    //StartWriteFileReq
    IN_HANDLE_ID,
    WAIT_CMD,
    IN_NAME_LEN,
    IN_FILE_NAME,
    IN_TOTAL_LEN,

    //StartWriteFileResp
    //IN_HANDLE_ID,
    //WAIT_CMD
    WAIT_STATUS_CODE,

    //SendChunkReq
    //StartWriteFileResp
    //IN_HANDLE_ID,
    //WAIT_CMD
    IN_CHUNK_ID,
    IN_START,
    IN_CHUNK_LEN,
    IN_CHUNK,

    //SendChunkResp
    //StartWriteFileResp
    //IN_HANDLE_ID,
    //WAIT_CMD
    //IN_CHUNK_ID,
    //WAIT_STATUS_CODE,

    WAIT_END,


}


type FWReq = fn(Arc<Mutex<ProtocolParser>>,u32, String,u64)->Result<u8>;
type FWResp = fn(Arc<Mutex<ProtocolParser>>,u32,Option<u8>);
type CWReq = fn(Arc<Mutex<ProtocolParser>>,u32,u32,usize,usize,&[u8])->Result<u8>;
type CWResp = fn(Arc<Mutex<ProtocolParser>>,u32,Option<u8>);
pub struct ChunkParser<'a>
{
    state:GL_CHUNK_STATE,
    cur_data_pos:usize,
    cmd:CMD,
    file_name_len_buf:Vec<u8>,
    file_name_len:u32,
    handle_id:u32,
    handle_id_buf:Vec<u8>,
    file_name_buf:Vec<u8>,
    chunk_id:u32,
    chunk_id_buf:Vec<u8>,
    total_len:u64,
    total_len_buf:Vec<u8>,
    chunk_start:u64,
    chunk_start_buf:Vec<u8>,
    chunk_len:u64,
    chunk_len_buf:Vec<u8>,
    status :Option<u8>,
    with_error:bool,
    callback:SyncSender<ProtocolMsgs<'a>>,

}
impl<'a> ChunkParser<'a>
{
    pub fn new(callback:SyncSender<ProtocolMsgs<'a>>)->Self{
        ChunkParser{
            state:GL_CHUNK_STATE::WAIT_START,
            cur_data_pos:0,
            cmd:CMD::UNKNOWN,
            file_name_len :0,
            handle_id:0,
            handle_id_buf:Vec::new(),
            file_name_len_buf:Vec::new(),
            file_name_buf:Vec::new(),
            chunk_id:0,
            chunk_id_buf:Vec::new(),
            total_len:0,
            total_len_buf:Vec::new(),
            chunk_start:0,
            chunk_start_buf:Vec::new(),
            chunk_len:0,
            chunk_len_buf:Vec::new(),
            status:None,
            with_error:false,
            callback:callback.clone(),
        }
    }
    pub fn reset(&mut self){
        self.state =GL_CHUNK_STATE::WAIT_START;
        self.cur_data_pos = 0;
        self.cmd = CMD::UNKNOWN;
        self.handle_id = 0;
        self.handle_id_buf = Vec::new();
        self.file_name_len = 0;
        self.file_name_len_buf = Vec::new();
        self.file_name_buf = Vec::new();
        self.chunk_id = 0;
        self.chunk_id_buf = Vec::new();
        self.total_len = 0;
        self.total_len_buf = Vec::new();
        self.chunk_len = 0;
        self.chunk_len_buf = Vec::new();
        self.status = None;
        self.chunk_start = 0;
        self.chunk_start_buf = Vec::new();
        self.with_error = false;
    }
    pub fn on_data(&mut self,in_char:u8,buffer:Option<&'a [u8]>)->u64{
        let mut ret = 0;
        match self.state {
            GL_CHUNK_STATE::WAIT_START =>{
                if in_char  == P_TAG::TAG_START as u8 {
                    self.cur_data_pos = 0;
                    self.state = GL_CHUNK_STATE::IN_HANDLE_ID
                }
            },
            GL_CHUNK_STATE::IN_HANDLE_ID=>{
                self.handle_id_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= std::mem::size_of::<u64>() {
                    let mut rdr = Cursor::new(&self.handle_id_buf[..]);
                    self.handle_id = rdr.read_u32::<BigEndian>().unwrap();
                    self.state = WAIT_CMD
                }
            },
            GL_CHUNK_STATE::WAIT_CMD=>{
                match num::FromPrimitive::from_u8(in_char).unwrap() {
                    CMD::WRITE_FILE_REQ =>{
                        self.state = GL_CHUNK_STATE::IN_NAME_LEN;
                        self.cmd = num::FromPrimitive::from_u8(in_char).unwrap() ;
                        self.cur_data_pos = 0;
                    },
                    CMD::WRITE_FILE_RESP =>{
                        self.state = GL_CHUNK_STATE::WAIT_STATUS_CODE;
                        self.cmd =  num::FromPrimitive::from_u8(in_char).unwrap() ;
                    },
                    CMD::WRITE_CHUNK_REQ=>{
                        self.state = GL_CHUNK_STATE::IN_CHUNK_ID;
                        self.cmd =  num::FromPrimitive::from_u8(in_char).unwrap() ;
                        self.cur_data_pos = 0;
                    },
                    CMD::WRITE_CHUNK_RESP=>{
                        self.state = GL_CHUNK_STATE::IN_CHUNK_ID;
                        self.cmd =  num::FromPrimitive::from_u8(in_char).unwrap() ;
                        self.cur_data_pos = 0
                    },
                    _=>{
                        //error recovery
                        self.state = GL_CHUNK_STATE::WAIT_START;
                    },
            }
            },
            GL_CHUNK_STATE::IN_NAME_LEN=>{
                self.file_name_len_buf.push(in_char);
                self.cur_data_pos += 1;

                if self.cur_data_pos >= std::mem::size_of::<u32>(){
                    let mut rdr = Cursor::new(&self.file_name_len_buf[..]);
                    self.file_name_len = rdr.read_u32::<BigEndian>().unwrap();
                    self.state = GL_CHUNK_STATE::IN_FILE_NAME;
                    self.cur_data_pos = 0;
                }
            },
            GL_CHUNK_STATE::IN_FILE_NAME=>{
                self.file_name_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= self.file_name_len as usize {
                    self.state = GL_CHUNK_STATE::IN_TOTAL_LEN;
                    self.cur_data_pos = 0;
                }
            },
            GL_CHUNK_STATE::IN_TOTAL_LEN=> unsafe {
                self.total_len_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= std::mem::size_of::<u64>()  {
                    let mut rdr = Cursor::new(&self.total_len_buf[..]);
                    self.total_len = rdr.read_u64::<BigEndian>().unwrap();
                    self.state = GL_CHUNK_STATE::WAIT_END;
                    self.cur_data_pos = 0;
                }
            },
            GL_CHUNK_STATE::WAIT_STATUS_CODE=>{
                self.status = Some(in_char);
                self.state = GL_CHUNK_STATE::WAIT_END;
            },
            GL_CHUNK_STATE::IN_CHUNK_ID=>{
                self.chunk_id_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= std::mem::size_of::<u32>() {
                    let mut rdr = Cursor::new(&self.chunk_id_buf[..]);
                    self.chunk_id = rdr.read_u32::<BigEndian>().unwrap();
                    if CMD::WRITE_CHUNK_REQ == num::FromPrimitive::from_u8(in_char).unwrap() {
                        self.state = GL_CHUNK_STATE::IN_START;
                        self.cur_data_pos = 0;
                    }else{
                        self.state = WAIT_STATUS_CODE;
                    }
                }
            },
            GL_CHUNK_STATE::IN_START=>{
                self.chunk_start_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= std::mem::size_of::<u64>() {
                    let mut rdr = Cursor::new(&self.total_len_buf[..]);
                    self.chunk_start = rdr.read_u64::<BigEndian>().unwrap();
                    self.state = GL_CHUNK_STATE::IN_CHUNK_LEN;
                    self.cur_data_pos = 0;
                }
            },
            GL_CHUNK_STATE::IN_CHUNK_LEN=>{
                self.chunk_len_buf.push(in_char);
                self.cur_data_pos += 1;
                if self.cur_data_pos >= std::mem::size_of::<u64>() {
                    let mut rdr = Cursor::new(&self.total_len_buf[..]);
                    self.chunk_len = rdr.read_u64::<BigEndian>().unwrap();
                    self.state = GL_CHUNK_STATE::IN_CHUNK_LEN;
                    self.cur_data_pos = 0;
                    ret = self.chunk_len;
                }
            },
            GL_CHUNK_STATE::IN_CHUNK=>{
                if let Some(buffer) = buffer {
                    if !self.with_error{
                        let chunk_req = WriteChunkReq {
                            handle:self.handle_id,
                            chunkid:self.chunk_id,
                            start:self.chunk_start + self.cur_data_pos as u64,
                            len:buffer.len() as u64,
                            chunk:Arc::new(Mutex::new(buffer))
                        };
                        self.status = if let Ok(v) = self.callback.send(ProtocolMsgs::<'a>::ChunkReq(chunk_req)){
                            None
                        }else{
                            self.with_error = true;
                            Some(0)
                        };
                    }

                    self.cur_data_pos += buffer.len();
                    if self.cur_data_pos >= self.chunk_len as usize{
                        self.state = WAIT_END;
                    }
                }else{
                    let mut to_char=[0u8;1];
                    to_char[0] = in_char;
                    if !self.with_error {
                        let chunk_req = WriteChunkReq {
                            handle:self.handle_id,
                            chunkid:self.chunk_id,
                            start:self.chunk_start + self.cur_data_pos as u64,
                            len:buffer.unwrap().len() as u64,
                            chunk:Arc::new(Mutex::new(&to_char[..]))
                        };
                        self.status = if let Ok(v) = self.callback.send(ProtocolMsgs::ChunkReq(chunk_req)) {
                            None
                        } else {
                            self.with_error = true;
                            Some(0)
                        };
                    }
                    self.cur_data_pos += 1;
                    if self.cur_data_pos >= self.chunk_len as usize{
                        self.state = WAIT_END;
                    }
                }
            }
            GL_CHUNK_STATE::WAIT_END=>{
                if in_char == P_TAG::TAG_END as u8 {
                    match self.cmd {
                        CMD::WRITE_FILE_REQ =>{

                            let file_req = WriteFileReq {
                                handle:self.handle_id,
                                file_name:String::from_utf8((&self).file_name_buf.clone()).unwrap(),
                                total_len:self.total_len
                            };
                            self.status = if let Ok(val) =self.callback.send(ProtocolMsgs::FileReq(file_req)){
                                None
                            }else{
                                Some(0)
                            }
                        }
                        CMD::WRITE_FILE_RESP =>{
                            let file_resp = WriteFileResp {
                                handle:self.handle_id,
                                status:if let Some(v) = self.status {
                                    v
                                }else{
                                    0
                                }

                            };
                            self.callback.send(ProtocolMsgs::FileResp(file_resp));
                        }
                        CMD::WRITE_CHUNK_REQ =>{
                        }
                        CMD::WRITE_CHUNK_RESP =>{
                            let chunk_resp = WriteChunkResp {
                                handle:self.handle_id,
                                chunkid:self.chunk_id,
                                status:if let Some(v) = self.status {
                                    v
                                }else{
                                    0
                                }


                            };
                            self.callback.send(ProtocolMsgs::ChunkResp(chunk_resp));
                        }
                        _ => {}
                    }
                }
            }
        }
        ret
    }
}

pub struct ProtocolParser<'a> {
    buf_size:usize,

    state: GL_STATE,
    buff_mode_left:u64,
    cmd_parser:Option<ChunkParser<'a>>,
    reader:Arc<Mutex<Option<Box<dyn Read + 'static + Send>>>>,
    writer:Arc<Mutex<Option<Box<Write + 'static + Send>>>>,


}


///|S|handle_id|CMD|namelen|file_name|total_len|END |
// |--|:-|:--|:--|:--|:--| -- |
// |0xBE|(u32)|(u8)|(u32)|string|(u64BE)|$ |
const WriteFileReqPrefix:u32 =  1 + 4 + 1 + 4 + 8 + 1;
///|S|handle_id|CMD|status_code|END |
// |--|:-|:--|:--| -- |
// |0xEB|(u32)|u8|u8|$ |
const WriteFileRespPrefix:u32 =  1 + 4 + 1  + 1 + 1;
///|S|handle_id|CMD|chunk_id|start|len|chunk|END |
// | -- | :- | :- -| :- -| :- -| :-- | -- | -- |
// |0xBE|(u32)|(u8)|(u32)|(u64BE)|(u64BE)|buffer|$ |
const WriteChunkReqPrefix:u32 =  1 + 4 + 1 + 4 + 8 + 8 +1;
///|S|handle_id|CMD|chunk_id|status_code|END |
// |--|:-|:--|:--|:--| -- |
// |0xEB|(u32)|(u8)|(u32)|u8|$ |
const WriteChunkRespPrefix:u32 = 1 + 4  + 1 + 4 + 1 +1 ;


impl<'a> ProtocolParser<'a>
{
    pub fn new(buf_size:usize)->Self{
        ProtocolParser{
            buf_size:buf_size,

            state:GL_STATE::IN_BYTE_MODE,
            buff_mode_left:0,
            cmd_parser:None,
            reader:Arc::new(Mutex::new(None)),
            writer:Arc::new(Mutex::new(None)),

        }
    }

    pub fn build(&mut self,msg_sender:SyncSender<ProtocolMsgs<'a>>){
        self.cmd_parser = Some(ChunkParser::new(msg_sender.clone()));
    }
    fn on_char(&mut self,in_char:u8){
         if let Some(cmd_parser) = &mut self.cmd_parser{
            let buf_mod_left = cmd_parser.on_data(in_char,None);
            if  buf_mod_left != 0 {
                self.buff_mode_left = buf_mod_left;
                self.state = GL_STATE::IN_BUFFER_MODE;
            }

        }else{
            panic!("no command parser!");
        }
    }

    //buffer processed data
    fn on_buffer(&mut self,in_buffer:&'a [u8])->u64{
        if let Some(cmd_parser) = &mut self.cmd_parser {
            if in_buffer.len() >= self.buff_mode_left as usize {
                cmd_parser.on_data(0, Some(&in_buffer[..self.buff_mode_left as usize]));
                self.state = GL_STATE::IN_BYTE_MODE;
                let processed = self.buff_mode_left;
                self.buff_mode_left = 0;
                processed
            } else {
                cmd_parser.on_data(0, Some(in_buffer));
                self.buff_mode_left -= in_buffer.len() as u64;
                in_buffer.len() as u64
            }
        }else{
            panic!("no command parser!");
        }
    }
    pub fn run(&mut self, reader:Arc<Mutex<impl Read + 'static + Send>>, writer:Arc<Mutex<Option<Box<Write + 'static + Send>>>>){
        let mut read_buffer = vec![0u8;self.buf_size];
        self.writer = writer.clone();

        loop {
            let size_value= {
                reader.lock().unwrap().deref_mut().read(&mut read_buffer[..])
            };
            if let Ok(size_value) = size_value {
                let mut processed = 0usize;

                while processed < size_value {
                    if self.state == GL_STATE::IN_BYTE_MODE {
                        self.on_char(read_buffer[processed]);
                        processed += 1;
                    }else{
                        let proced = self.on_buffer(&read_buffer[processed..]);
                        processed += proced as usize;
                    }
                }

                if size_value == 0 {
                    break;
                }
            }else{
                format!("read buffer error");
            }

        }
    }


}

pub fn create_write_req(file_name:String,data_len:u64)->Result<(u32,Vec<u8>)>{
    let buffer_len = WriteFileReqPrefix + file_name.len() as u32;
    let mut buffer = Vec::with_capacity(buffer_len as usize);
    //create handle_id
    let handle_id = rand::random::<u32>();
    buffer[0] = TAG_START as u8 ;
    (&mut buffer[1..]).write_u32::<BigEndian>(handle_id).unwrap();
    buffer[1+4] = CMD::WRITE_FILE_REQ as u8;
    (&mut buffer[6..]).write_u32::<BigEndian>(buffer_len as u32 ).unwrap();
    &mut buffer[10..10+buffer_len as usize].clone_from_slice(&file_name.as_bytes());
    (&mut buffer[10+buffer_len as usize..]).write_u64::<BigEndian>(data_len).unwrap();
    buffer[18+buffer_len as usize] = TAG_END as u8;

    Ok((handle_id,buffer))


}

pub fn create_write_resp(handle_id:u32,status:u8)->Result<Vec<u8>>{
    let buffer_len = WriteFileRespPrefix as usize;
    let mut buffer = Vec::with_capacity(buffer_len);
    //create handle_id

    buffer[0] = TAG_START as u8 ;
    (&mut buffer[1..]).write_u32::<BigEndian>(handle_id).unwrap();
    buffer[1+4] = CMD::WRITE_FILE_REQ as u8;
    buffer[6] = status;
    buffer[7] = TAG_END as u8;

    Ok(buffer)
}

pub fn create_chunk_req(mut reader :impl Read, handle_id:u32, chunk_id:u32, start:u64, len:u64) ->Result<Vec<u8>>{
    let buffer_len = WriteChunkRespPrefix as u64+len;
    let mut buffer = vec![0u8;buffer_len as usize];
    //create handle_id

    buffer[0] = TAG_START as u8 ;
    (&mut buffer[1..]).write_u32::<BigEndian>(handle_id).unwrap();
    buffer[1+4] = CMD::WRITE_CHUNK_REQ as u8;
    (&mut buffer[6..]).write_u32::<BigEndian>(chunk_id).unwrap();
    (&mut buffer[10..]).write_u64::<BigEndian>(start).unwrap();
    if let Ok(size) = (&mut reader).read(&mut buffer[26..26+len as usize]) {
        (&mut buffer[18..]).write_u64::<BigEndian>(size as u64).unwrap();
        buffer[26+size as usize] = TAG_END as u8;
        Ok(buffer)
    }else{
        Err(anyhow!("error in reading file"))
    }

}

pub fn create_chunk_resp(handle_id:u32,chunk_id:u32,status:u8)->Result<Vec<u8>>{
    let buffer_len = WriteChunkRespPrefix as usize;
    let mut buffer = Vec::with_capacity(buffer_len);
    //create handle_id

    buffer[0] = TAG_START as u8 ;
    (&mut buffer[1..]).write_u32::<BigEndian>(handle_id).unwrap();
    buffer[1+4] = CMD::WRITE_CHUNK_RESP as u8;
    (&mut buffer[6..]).write_u32::<BigEndian>(chunk_id).unwrap();
    buffer[10] = status;
    buffer[11] = TAG_END as u8;

    Ok(buffer)
}

