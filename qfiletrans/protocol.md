# Description
**QFile** a very simple file transfer protocol, which can send data from client to server and resuming from broken

#Protocols 
* StartWriteFileReq  

|S|handle_id|CMD|namelen|file_name|total_len|END |  
|--|:-|:--|:--|:--|:--| -- |  
|0xBE|(u32)|(u8)|(u32)|string|(u64BE)|$ |

* StartWriteFileResp

|S|handle_id|CMD|status_code|END |  
|--|:-|:--|:--| -- |  
|0xEB|(u32)|u8|u8|$ |

status_code: 0x00 success 0x01 out_of_space, 0x02, out_of_memory, others os error

* SendChunksReq

|S|handle_id|CMD|chunk_id|start|len|chunk|END |  
| -- | :- | :- -| :- -| :- -| :-- | -- | -- |  
|0xBE|(u32)|(u8)|(u32)|(u64BE)|(u64BE)|buffer|$ |


* SendChunkResp  

|S|handle_id|CMD|chunk_id|status_code|END |  
|--|:-|:--|:--|:--| -- |  
|0xEB|(u32)|(u8)|(u32)|u8|$ |

* StatFileReq   
*TODO* used to persist file transfer state

* StatFileResp  
*TODO* used to persist file transfer state
