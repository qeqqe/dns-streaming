#![allow(dead_code, unused_variables, unused_mut)]
use std::{error::Error, net::UdpSocket};

use crate::transcoder::Transcoder;
mod transcoder;

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("127.0.0.1:5300").unwrap();
    let mut buf = [0u8; 512];

    let mut ts = Transcoder::new(
        "/home/qeqqer/Watch-List/jjk/53.mkv".into(),
        "./new_file.mkv".into(),
    );

    let _ = ts.chunk_video();

    loop {
        let (len, addr) = socket.recv_from(&mut buf).unwrap();
        let request = &buf[..len];

        let id = &request[0..2];
        let query = &request[12..];

        let name = parse_query(query);
        let chunk_number = get_chunk(&name);

        println!("name: {:?}, chunk_number: {:?}", name, chunk_number);
    }
}

fn is_valid_query(query: &str) -> Result<(), Box<dyn Error>> {
    let len = query.len();
    if query.get(0..6).unwrap() == "chunk-" && query.get(len - 7..len).unwrap() == ".local" {
        Ok(())
    } else {
        Err("Invalid Format, Valid format: 'chunk-[chunk-number].local'".into())
    }
}

fn parse_query(query: &[u8]) -> String {
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
/// always starts at 6th (includede) char and ends on len - 6th char (excluded)
/// `chunk-43.local`
fn get_chunk(query: &str) -> usize {
    if is_valid_query(query).is_ok() {
        panic!("Invalid query: {query}");
    }
    let len = query.len();
    let chunk_str = query.get(6..len - 6).unwrap();
    chunk_str.parse().unwrap()
}
