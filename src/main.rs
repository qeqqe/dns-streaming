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

    let mut ts = Transcoder::new("/home/qeqqer/Watch-List/jjk/Jujutsu Kaisen - 54.mkv".into());

    let _ = ts.chunk_video();

    loop {
        let (len, addr) = server.socket.recv_from(&mut buf).await.unwrap();
        let request = &buf[..len];

        let (chunk_number, name) = server.parse_request(request);

        let chunk: &Vec<PacketData> = ts.get_chunk(chunk_number).unwrap();

        let req_body_len = buf[12..].len();
        const ANSWER_HEADER_LEN: usize = 12;

        // fragmentation
        let chunk_bytes_size = chunk.iter().map(|p| p.pkt_len + 4).sum::<usize>();
        if chunk_bytes_size <= 65507 - req_body_len - ANSWER_HEADER_LEN {
            let chunk_bytes = server.construct_response(request, chunk);
            println!("chunk len:{}", chunk_bytes.len());

            server.socket.send_to(&chunk_bytes, addr).await.unwrap();
        } else {
            let fragmented_response = server.construct_fragmented_response(request, chunk);

            for chunk_bytes in fragmented_response {
                println!("fragmented chunk len:{}", chunk_bytes.len());
                server.socket.send_to(&chunk_bytes, addr).await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        }
    }
}
