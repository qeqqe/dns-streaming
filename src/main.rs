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

    let mut ts = Transcoder::new(
        "/home/qeqqer/Downloads/drive-download-20260510T191742Z-3-001/Vinland Saga - S01E11.mkv"
            .into(),
    );

    let _ = ts.chunk_video();

    loop {
        let (len, addr) = server.socket.recv_from(&mut buf).await.unwrap();
        let request = &buf[..len];

        let (chunk_number, name) = server.parse_request(request);

        // println!("request {:#?}", &request[0..]);

        let chunk: &Vec<PacketData> = ts.get_chunk(chunk_number).unwrap();

        let total_chunk_len = chunk.iter().map(|packet| packet.pkt_len).sum::<usize>();

        // fragmentation
        if total_chunk_len <= 65507 {
            let chunk_bytes = server.construct_response(request, chunk);

            server.socket.send_to(&chunk_bytes, addr).await.unwrap();
        } else {
            let fragmented_response = server.construct_fragmented_response(request, chunk);

            for chunk_bytes in fragmented_response {
                server.socket.send_to(&chunk_bytes, addr).await.unwrap();
            }
        }
    }
}
