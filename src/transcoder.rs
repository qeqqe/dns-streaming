use ffmpeg_next::format;

use std::{error::Error, mem, path::PathBuf};

const MAX_UDP_PAYLOAD: usize = 65_507;

pub struct Transcoder {
    media_path: PathBuf,
    chunk_path: PathBuf,
}

#[derive(Debug)]
pub struct PacketData {
    pkt_len: usize,
    pkt_data: Vec<u8>,
    is_key: bool,
}

impl Transcoder {
    pub fn new(media_path: PathBuf, chunk_path: PathBuf) -> Self {
        Self {
            media_path,
            chunk_path,
        }
    }

    pub fn chunk_video(&mut self) -> Result<(), Box<dyn Error>> {
        ffmpeg_next::init().unwrap();
        let mut ictx = format::input(&self.media_path)?;
        let mut octx = format::output(&self.chunk_path)?;

        // this will store a valid frames and make sure it's not in middle of GOP
        let mut packets_array: Vec<Vec<PacketData>> = vec![];

        // accumulates till length threshold (MAX_UDP_PAYLOAD) and
        // make sure that the last packet is a key.
        let mut accumulate_packet: Vec<PacketData> = vec![];

        let mut cur_size: usize = 0;

        let mut is_fragmented_now = false;

        println!("{}", ictx.bit_rate());
        for (_, mut packets) in ictx.packets() {
            // these are usually the stream's av metadata
            // that we're going to ignore
            if packets.size() == 0 {
                continue;
            }

            // 2 bytes is for the u16 size pkt_len which
            // covers all size till 65535
            // [pkt_len] [pkt_data] | [pkt_len] [pkt_data] ...
            if cur_size + 2 + packets.size() > MAX_UDP_PAYLOAD {
                let (mut non_key_size, mut last_key_idx) =
                    self.get_last_key_frame(&accumulate_packet);

                // what if there's no last keyframe in the accumulate?? [is_key: false, is_key: false...]
                // last_key_idx will be 0, WHICH means that empty packets will be stored INFINITELY.
                if last_key_idx == 0 && !accumulate_packet.is_empty() {
                    non_key_size = accumulate_packet.len();
                    last_key_idx = 0;
                }

                // only push when we got something valid to push
                if last_key_idx > 0 {
                    packets_array.push(accumulate_packet.drain(0..last_key_idx).collect());
                    cur_size = non_key_size;
                }
            }

            // for packets that exceed the max paylod len we transmit it as a
            // fragmented chunk and send it with a fragment flag over UDP
            // and let the client handle the fragmented packet and reconstruct it.
            if packets.size() > MAX_UDP_PAYLOAD {
                // no neeed for calculating the previous I-keyframe
                // packet, already chunked it together just
                // accumulate till next keyframe.
                is_fragmented_now = true;

                accumulate_packet.push(PacketData {
                    pkt_len: packets.size(),
                    pkt_data: (*packets.data().unwrap()).to_vec(),
                    is_key: packets.is_key(),
                });

                cur_size += 2 + packets.size();
                if packets.is_key() {
                    packets_array.push(mem::take(&mut accumulate_packet));
                    accumulate_packet = vec![];
                    is_fragmented_now = false;
                    cur_size = 0;
                }
            } else if is_fragmented_now {
                accumulate_packet.push(PacketData {
                    pkt_len: packets.size(),
                    pkt_data: (*packets.data().unwrap()).to_vec(),
                    is_key: packets.is_key(),
                });

                cur_size += 2 + packets.size();

                if packets.is_key() {
                    is_fragmented_now = false;
                    cur_size = 0;
                    packets_array.push(mem::take(&mut accumulate_packet));
                    accumulate_packet = vec![];
                }
            }

            if cur_size + 2 + packets.size() <= MAX_UDP_PAYLOAD {
                accumulate_packet.push(PacketData {
                    pkt_len: packets.size(),
                    pkt_data: (*packets.data().unwrap()).to_vec(),
                    is_key: packets.is_key(),
                });
                cur_size += 2 + packets.size();
            }
        }

        if !accumulate_packet.is_empty() {
            println!("Extra appending fn triggered");
            // last element must be a key_frame
            packets_array.push(mem::take(&mut accumulate_packet));
        }

        for (n, pd) in packets_array.iter().enumerate() {
            let mut len_acc: usize = 0;
            for packets in pd {
                len_acc += packets.pkt_len;
            }
            println!(
                "Chunk {}, storing: {} bytes, packets: {}",
                n,
                len_acc,
                pd.len()
            );
            if pd.len() == 3 && len_acc == 144777
                || pd.len() == 33 && len_acc == 61247
                || len_acc == 52050 && pd.len() == 33
            {
                for (num, packet) in pd.iter().enumerate() {
                    println!(
                        "packet {} len: {:#?}, is_key: {}",
                        num, packet.pkt_len, packet.is_key
                    );
                }
            }
            if len_acc > MAX_UDP_PAYLOAD {
                println!("needs fragmenting!")
            }
        }

        Ok(())
    }

    pub fn get_last_key_frame(&mut self, accumulate: &Vec<PacketData>) -> (usize, usize) {
        if accumulate.is_empty() {
            return (0, 0);
        }
        let mut non_key_size: usize = 0;
        let mut last_key_idx: usize = accumulate.len();

        for packets in accumulate.iter().rev() {
            // packets.is_key() tells that packet IS a keyframe (I-frame).
            // meaning the decoder can start fresh from this point
            // without needing any previous frames.
            // so only push the frames when its a key.

            if packets.is_key {
                break;
            } else {
                non_key_size += 2 + packets.pkt_len;
                last_key_idx = last_key_idx.saturating_sub(1);
            }
        }

        (non_key_size, last_key_idx)
    }
}
