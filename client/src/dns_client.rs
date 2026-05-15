use std::error::Error;

use tokio::net::UdpSocket;

pub struct DNSClient {
    socket: UdpSocket,
    dns_server_ip: String,
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
    pub async fn request_chunk(&mut self, chunk_number: usize) -> Result<Vec<u8>, Box<dyn Error>> {
        let request = self.create_dns_request(chunk_number);

        self.socket
            .send_to(&request, &self.dns_server_ip)
            .await
            .unwrap();

        let mut buf = [0u8; 65536];

        let (len, src) = self.socket.recv_from(&mut buf).await.unwrap();
        println!("response:\n\n{:#?}", &buf[0..len]);

        todo!()
    }

    fn create_dns_request(&mut self, chunk_number: usize) -> Vec<u8> {
        let mut request: Vec<u8> = vec![];
        let chunk_num_len = format!("{}", chunk_number).len();
        let chunk_len = format!("{:02x}", chunk_num_len + 6).parse::<u8>().unwrap(); // "chunk-".len() = 6  
        let chunk_ident = format!("chunk-{}", chunk_number).into_bytes();
        let local_ident = "local".to_string().into_bytes();

        println!("chunk len: {}", chunk_len);

        // header
        request.extend_from_slice(b"\xaa\xbb"); // ID
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
