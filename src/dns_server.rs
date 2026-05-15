use std::error::Error;

use tokio::net::UdpSocket;

use crate::transcoder::PacketData;

pub struct DNSServer {
    pub socket: UdpSocket,
}

impl DNSServer {
    pub async fn start(addr: String) -> Self {
        let socket = UdpSocket::bind(addr).await.unwrap();
        Self { socket }
    }

    // aa bb             - ID
    // 01 00             - Flags (standard query)
    // 00 01             - QDCOUNT (1 question)
    // 00 00             - ANCOUNT
    // 00 00             - NSCOUNT
    // 00 00             - ARCOUNT
    //
    // 08                - label length (8)
    // 63 68 75 6e 6b 2d 34 33  - "chunk-43"
    // 05               - label length (5)
    // 6c 6f 63 61 6c   - "local"
    // 00               - null terminator
    // 00 01            - QTYPE (A record)
    // 00 01            - QCLASS (IN)
    pub fn parse_request(&mut self, request: &[u8]) -> (usize, String) {
        let name = self.parse_query(&request[12..]);
        (self.get_chunk(&name), name)
    }

    // aa bb          - ID (echoed)
    // 81 80          - Flags (response, recursion available)
    // 00 01          - QDCOUNT
    // 00 01          - ANCOUNT
    // 00 00          - NSCOUNT
    // 00 00          - ARCOUNT
    //
    // question section echoed verbatim:
    // 08 63 68 75 6e 6b 2d 34 33 05 6c 6f 63 61 6c 00 00 01 00 01
    //
    // answer section:
    // c0 0c          - name pointer to offset 12
    // 00 01          - TYPE A
    // 00 01          - CLASS IN
    // 00 00 00 00    - TTL
    // [2 bytes]      - RDLENGTH = chunk payload length
    // [chunk bytes]  - actual data
    pub fn construct_response(&mut self, request: &[u8], chunk: &Vec<PacketData>) -> Vec<u8> {
        let mut response: Vec<u8> = vec![];

        // header
        response.extend_from_slice(&request[0..2]); // id
        response.extend_from_slice(&[0x81, 0x80]); // flags
        response.extend_from_slice(&[0x00, 0x01]); // QDCOUNT
        response.extend_from_slice(&[0x00, 0x01]); // ANCOUNT
        response.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
        response.extend_from_slice(&[0x00, 0x00]); // ARCOUNT

        // question verbaitim
        response.extend_from_slice(&request[12..]);

        // answer header
        response.extend_from_slice(&[0xc0, 0x0c]); // name pointer
        response.extend_from_slice(&[0x00, 0x01]); // TYPE A
        response.extend_from_slice(&[0x00, 0x01]); // CLASS IN
        response.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // TTL

        // answer
        let mut chunk_bytes: Vec<u8> = vec![];
        for packets in chunk {
            let len_bytes = (packets.pkt_len as u16).to_be_bytes();
            println!("len bytes: {:#?}", len_bytes);

            chunk_bytes.extend_from_slice(&len_bytes);
            chunk_bytes.extend_from_slice(&packets.pkt_data);
        }
        println!("chunk bytes first: {:#?}", chunk_bytes.get(0..5).unwrap());

        let rdlength = chunk_bytes.len() as u16;
        println!("rdlen: {rdlength}");
        response.extend_from_slice(&rdlength.to_be_bytes()); // RDLENGTH
        println!("rdlen bytes: {:#?}", &rdlength.to_be_bytes());
        response.extend_from_slice(&chunk_bytes);

        println!("{:#?}", response.get(0..12).unwrap());

        response
    }

    fn is_valid_query(&mut self, query: &str) -> Result<(), Box<dyn Error>> {
        let len = query.len();
        if query.get(0..6).unwrap() == "chunk-" && query.get(len - 7..len).unwrap() == ".local" {
            Ok(())
        } else {
            Err("Invalid Format, Valid format: 'chunk-[chunk-number].local'".into())
        }
    }

    fn parse_query(&mut self, query: &[u8]) -> String {
        let mut lables = vec![];
        let mut i: usize = 0;
        let q_len = query.len();
        while query[i] != 0 {
            let mut len = query[i] as usize;
            i += 1;

            lables.push(String::from_utf8_lossy(&query[i..i + len]));
            i += len;
        }

        lables.join(".")
    }

    /// format for chunking
    /// always starts at 6th (included) char and ends on len - 6th char (excluded)
    /// `chunk-43.local`
    fn get_chunk(&mut self, query: &str) -> usize {
        if self.is_valid_query(query).is_ok() {
            panic!("Invalid query: {query}");
        }
        let len = query.len();
        let chunk_str = query.get(6..len - 6).unwrap();
        chunk_str.parse().unwrap()
    }
}
