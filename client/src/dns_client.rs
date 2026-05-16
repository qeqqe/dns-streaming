use std::error::Error;

use tokio::net::UdpSocket;

pub struct DNSClient {
    socket: UdpSocket,
    dns_server_ip: String,
}

pub struct ChunkData {
    pub packet_bytes: Vec<PacketData>,
}

#[derive(Debug, Clone)]
pub struct PacketData {
    pub pkt_len: usize,
    pub pkt_data: Vec<u8>,
}

impl DNSClient {
    pub async fn get_client(server_ip: String) -> Self {
        Self {
            socket: UdpSocket::bind("0.0.0.0:0").await.unwrap(), // ephemeral port
            dns_server_ip: server_ip,
        }
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
    pub async fn request_chunk(
        &mut self,
        chunk_number: usize,
    ) -> Result<ChunkData, Box<dyn Error>> {
        let request = self.create_dns_request(chunk_number);
        self.socket
            .send_to(&request, &self.dns_server_ip)
            .await
            .unwrap();

        let mut buf = [0u8; 65536];
        let expected_id = (chunk_number as u16).to_be_bytes();

        let chunk_num_len = format!("{}", chunk_number).len();

        const HEADER_OFFSET: usize = 12;
        const CHUNK_LIT_OFFSET: usize = 6;
        const LOCAL_LIT_OFFSET: usize = 5;
        const ANSWER_HEADER_OFFSET: usize = 15;
        const LEN_OFFSET: usize = 1;

        let rdlength_offset = HEADER_OFFSET
            + LEN_OFFSET
            + CHUNK_LIT_OFFSET
            + chunk_num_len
            + LEN_OFFSET
            + LOCAL_LIT_OFFSET
            + ANSWER_HEADER_OFFSET;

        let mut all_chunk_bytes: Vec<u8> = vec![];
        let mut receiving_fragments = false;

        loop {
            let recv_result = tokio::time::timeout(
                std::time::Duration::from_millis(1500),
                self.socket.recv_from(&mut buf),
            )
            .await;

            let (len, _src) = match recv_result {
                Ok(Ok(res)) => res,
                _ => return Err("Timeout or receive error".into()),
            };

            // ID check
            if buf[0] != expected_id[0] || buf[1] != expected_id[1] {
                continue; // Ignore stale packet
            }

            let flags = u16::from_be_bytes([buf[2], buf[3]]);
            let tc_flag = (flags >> 9) & 1;

            let rdlength =
                u16::from_be_bytes([buf[rdlength_offset], buf[rdlength_offset + 1]]) as usize;

            if tc_flag == 1 || receiving_fragments {
                receiving_fragments = true;
                all_chunk_bytes
                    .extend_from_slice(&buf[rdlength_offset + 2..rdlength_offset + 2 + rdlength]);

                if tc_flag == 0 {
                    return Ok(self.parse_request(&all_chunk_bytes, 0, all_chunk_bytes.len()));
                }
            } else {
                return Ok(self.parse_request(&buf, rdlength_offset + 2, rdlength));
            }
        }
    }

    fn parse_request(&mut self, buf: &[u8], offset: usize, chunk_len: usize) -> ChunkData {
        // layout: [pkt_len] [pkt_data] | [pkt_len] [pkt_data] | ...
        let chunk_bytes = &buf[offset..];

        let mut packet_bytes: Vec<PacketData> = vec![];

        let mut idx: usize = 0;

        while idx < chunk_len {
            if idx + 4 > chunk_len {
                println!(
                    "Parse error: not enough bytes for length prefix at idx {}",
                    idx
                );
                break;
            }

            // length is 4 bytes now!
            let pkt_len_bytes = &chunk_bytes[idx..=idx + 3];
            let pkt_len = u32::from_be_bytes([
                pkt_len_bytes[0],
                pkt_len_bytes[1],
                pkt_len_bytes[2],
                pkt_len_bytes[3],
            ]) as usize;

            idx += 4;

            if idx + pkt_len > chunk_len {
                println!(
                    "Parse error: pkt_len {} exceeds remaining bytes {}",
                    pkt_len,
                    chunk_len - idx
                );
                break;
            }

            let pkt_data = chunk_bytes[idx..idx + pkt_len].to_vec();
            let packet_data = PacketData { pkt_len, pkt_data };
            packet_bytes.push(packet_data);
            idx += pkt_len;
        }

        ChunkData { packet_bytes }
    }

    fn create_dns_request(&mut self, chunk_number: usize) -> Vec<u8> {
        let mut request: Vec<u8> = vec![];
        let chunk_num_len = format!("{}", chunk_number).len();
        let chunk_len = format!("{:02x}", chunk_num_len + 6).parse::<u8>().unwrap(); // "chunk-".len() = 6  
        let chunk_ident = format!("chunk-{}", chunk_number).into_bytes();
        let local_ident = "local".to_string().into_bytes();

        println!("chunk len: {}", chunk_len);

        // header
        let id = (chunk_number as u16).to_be_bytes();
        request.extend_from_slice(&id); // ID
        request.extend_from_slice(&[0x01, 0x00]); // flags
        request.extend_from_slice(&[0x00, 0x01]); // QDCOUNT
        request.extend_from_slice(&[0x00, 0x01]); // ANCOUNT
        request.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
        request.extend_from_slice(&[0x00, 0x00]); // ARCOUNT

        // question
        request.extend_from_slice(&[chunk_len]);
        request.extend_from_slice(&chunk_ident);
        request.push(b'\x05');
        request.extend_from_slice(&local_ident);
        request.push(b'\x00'); // null terminator
        request.extend_from_slice(&[0x00, 0x01]); // qtype
        request.extend_from_slice(&[0x00, 0x01]); // qclass

        println!("{:#?}", request);

        request
    }
}
