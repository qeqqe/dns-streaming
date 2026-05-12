use std::net::UdpSocket;

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("127.0.0.1:5300").unwrap();
    let mut buf = [0u8; 512];

    loop {
        let (len, addr) = socket.recv_from(&mut buf).unwrap();
        let request = &buf[..len];

        let id = &request[0..2];
        let query = &request[12..];

        // aa bb        → Transaction ID (arbitrary, 0xAABB)
        // 01 00        → Flags: standard query, recursion desired
        // 00 01        → QDCOUNT: 1 question
        // 00 00        → ANCOUNT: 0 answers
        // 00 00        → NSCOUNT: 0 authority records
        // 00 00        → ARCOUNT: 0 additional records
        //
        // 03 77 77 77  → \x03 = length 3, then "www"
        // 06 676f6f676c65 → \x06 = length 6, then "google"
        // 03 636f6d   → \x03 = length 3, then "com"
        // 00           → null terminator for the name
        //
        // 00 01        → QTYPE: A record
        // 00 01        → QCLASS: IN (internet)

        let name = parse_query(query);
        println!("name: {:?}", name);
    }
}

fn parse_query(query: &[u8]) -> String {
    let mut lables = vec![];
    let mut i: usize = 0;
    while query[i] != 0 {
        let len = query[i] as usize;
        i += 1;

        lables.push(String::from_utf8_lossy(&query[i..i + len]));
        i += len;
    }

    lables.join(".")
}

// format for chunking
// `chunk-43.local`

