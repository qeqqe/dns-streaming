#![allow(dead_code, unused_variables, unused_mut)]
use crate::{
    dns_server::DNSServer,
    transcoder::{PacketData, Transcoder},
};
mod dns_server;
mod transcoder;

#[tokio::main]
async fn main() {
    let mut server = DNSServer::start("127.0.0.1:5300".into()).await;

    let mut buf = [0u8; 512];

    let mut ts =
        Transcoder::new("/home/qeqqer/Watch-List/jjk/[Kayoanime] Jujutsu Kaisen - 51.mkv".into());

    let _ = ts.chunk_video();

    loop {
        let (len, addr) = server.socket.recv_from(&mut buf).await.unwrap();
        let request = &buf[..len];

        let (chunk_number, name) = server.parse_request(request);

        println!("request {:#?}", &request[0..]);

        let chunk: &Vec<PacketData> = ts.get_chunk(chunk_number).unwrap();

        let chunk_bytes = server.construct_response(request, chunk);

        println!("Returning the chunk of size: {}", chunk_bytes.len());
        println!("first 2 bytes {:#?}", &chunk_bytes.get(0..2));

        if chunk_bytes.len() > 65507 {
            println!("Chunk size exceeds MAX_UDP_PAYLOAD (65507). Skipping for now.");
            continue;
        }

        server.socket.send_to(&chunk_bytes, addr).await.unwrap();

        println!("name: {:?}, chunk_number: {:?}", name, chunk_number);
    }
}
