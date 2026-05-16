use ffmpeg_next::format;

use std::{error::Error, mem, path::PathBuf};

fn convert_avcc_to_annex_b(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 8);
    let mut i = 0;
    while i + 4 <= data.len() {
        let nal_len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        i += 4;
        if i + nal_len > data.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[i..i + nal_len]);
        i += nal_len;
    }
    out
}

fn extract_extradata(extradata: &[u8], is_h265: bool) -> Vec<u8> {
    if extradata.is_empty() || extradata[0] != 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if is_h265 {
        if extradata.len() < 23 {
            return out;
        }
        let num_arrays = extradata[22];
        let mut offset = 23;
        for _ in 0..num_arrays {
            if offset + 3 > extradata.len() {
                break;
            }
            let num_nalus =
                u16::from_be_bytes([extradata[offset + 1], extradata[offset + 2]]) as usize;
            offset += 3;
            for _ in 0..num_nalus {
                if offset + 2 > extradata.len() {
                    break;
                }
                let nal_len =
                    u16::from_be_bytes([extradata[offset], extradata[offset + 1]]) as usize;
                offset += 2;
                if offset + nal_len > extradata.len() {
                    break;
                }
                out.extend_from_slice(&[0, 0, 0, 1]);
                out.extend_from_slice(&extradata[offset..offset + nal_len]);
                offset += nal_len;
            }
        }
    } else {
        if extradata.len() < 7 {
            return out;
        }
        let num_sps = extradata[5] & 0x1f;
        let mut offset = 6;
        for _ in 0..num_sps {
            if offset + 2 > extradata.len() {
                break;
            }
            let sps_len = u16::from_be_bytes([extradata[offset], extradata[offset + 1]]) as usize;
            offset += 2;
            if offset + sps_len > extradata.len() {
                break;
            }
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&extradata[offset..offset + sps_len]);
            offset += sps_len;
        }
        if offset >= extradata.len() {
            return out;
        }
        let num_pps = extradata[offset];
        offset += 1;
        for _ in 0..num_pps {
            if offset + 2 > extradata.len() {
                break;
            }
            let pps_len = u16::from_be_bytes([extradata[offset], extradata[offset + 1]]) as usize;
            offset += 2;
            if offset + pps_len > extradata.len() {
                break;
            }
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(&extradata[offset..offset + pps_len]);
            offset += pps_len;
        }
    }
    out
}

const MAX_UDP_PAYLOAD: usize = 65_507;

pub struct Transcoder {
    pub media_path: PathBuf,
    pub packet_array: Vec<Vec<PacketData>>,
}

#[derive(Debug, Clone)]
pub struct PacketData {
    pub pkt_len: usize,
    pub pkt_data: Vec<u8>,
    pub is_key: bool,
}

impl Transcoder {
    pub fn new(media_path: PathBuf) -> Self {
        Self {
            media_path,
            // this will store a valid frames and make sure it's not in middle of GOP
            packet_array: Vec::new(),
        }
    }

    pub fn chunk_video(&mut self) -> Result<(), Box<dyn Error>> {
        ffmpeg_next::init().unwrap();
        let mut ictx = format::input(&self.media_path)?;

        let video_stream_index = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or("no video stream")?
            .index();

        let sps_pps = {
            let stream = ictx.stream(video_stream_index).unwrap();
            let params = stream.parameters();
            let is_h265 = params.id() == ffmpeg_next::codec::Id::HEVC;
            let extradata = unsafe {
                let ptr = params.as_ptr();
                if (*ptr).extradata.is_null() || (*ptr).extradata_size <= 0 {
                    &[]
                } else {
                    std::slice::from_raw_parts((*ptr).extradata, (*ptr).extradata_size as usize)
                }
            };
            extract_extradata(extradata, is_h265)
        };

        // convert AVCC (MP4 length-prefixed) to Annex B (start codes)
        // so the raw H.264 decoder doesn't choke on missing start codes

        // accumulates till length threshold (MAX_UDP_PAYLOAD) and
        // make sure that the last packet is a key.
        let mut accumulate_packet: Vec<PacketData> = vec![];

        let mut cur_size: usize = 0;

        let mut is_fragmented_now = false;

        println!("{}", ictx.bit_rate());
        for (stream, packets) in ictx.packets() {
            if stream.index() != video_stream_index {
                continue;
            }

            // these are usually the stream's av metadata
            // that we're going to ignore
            if packets.size() == 0 {
                continue;
            }

            let is_key = packets.is_key();
            let raw_data = packets.data().unwrap();
            let mut pkt_data = convert_avcc_to_annex_b(raw_data);

            if is_key && !sps_pps.is_empty() {
                let mut new_pkt = Vec::with_capacity(sps_pps.len() + pkt_data.len());
                new_pkt.extend_from_slice(&sps_pps);
                new_pkt.extend_from_slice(&pkt_data);
                pkt_data = new_pkt;
            }

            let pkt_size = pkt_data.len();
            {
                // 4 bytes is for the u32 size pkt_len
                // [pkt_len] [pkt_data] | [pkt_len] [pkt_data] | ...
                if cur_size + 4 + pkt_size > MAX_UDP_PAYLOAD {
                    let (mut non_key_size, mut last_key_idx) =
                        self.get_last_key_frame(&accumulate_packet);

                    // If there's no keyframe in the buffer, we still MUST flush to avoid dropping frames!
                    if last_key_idx == 0 && !accumulate_packet.is_empty() {
                        self.packet_array.push(mem::take(&mut accumulate_packet));
                        cur_size = 0;
                    } else if last_key_idx > 0 {
                        self.packet_array
                            .push(accumulate_packet.drain(0..last_key_idx).collect());
                        cur_size = non_key_size;
                    }
                }

                // for packets that exceed the max paylod len we transmit it as a
                // fragmented chunk and send it with a fragment flag over UDP
                // and let the client handle the fragmented packet and reconstruct it.
                if pkt_size > MAX_UDP_PAYLOAD {
                    is_fragmented_now = true;

                    accumulate_packet.push(PacketData {
                        pkt_len: pkt_size,
                        pkt_data: pkt_data.clone(),
                        is_key,
                    });

                    cur_size += 4 + pkt_size;
                    if is_key {
                        self.packet_array.push(mem::take(&mut accumulate_packet));
                        accumulate_packet = vec![];
                        is_fragmented_now = false;
                        cur_size = 0;
                    }
                } else if is_fragmented_now {
                    accumulate_packet.push(PacketData {
                        pkt_len: pkt_size,
                        pkt_data: pkt_data.clone(),
                        is_key,
                    });

                    cur_size += 4 + pkt_size;

                    if is_key {
                        is_fragmented_now = false;
                        cur_size = 0;
                        self.packet_array.push(mem::take(&mut accumulate_packet));
                        accumulate_packet = vec![];
                    }
                }

                if cur_size + 4 + pkt_size <= MAX_UDP_PAYLOAD
                    && pkt_size <= MAX_UDP_PAYLOAD
                    && !is_fragmented_now
                {
                    accumulate_packet.push(PacketData {
                        pkt_len: pkt_size,
                        pkt_data,
                        is_key,
                    });
                    cur_size += 4 + pkt_size;
                }
            }
        }

        if !accumulate_packet.is_empty() {
            // flush remaining
            self.packet_array.push(mem::take(&mut accumulate_packet));
        }

        Ok(())
    }

    pub fn get_last_key_frame(&mut self, accumulate: &[PacketData]) -> (usize, usize) {
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
                non_key_size += 4 + packets.pkt_len;
                last_key_idx = last_key_idx.saturating_sub(1);
            }
        }

        (non_key_size, last_key_idx)
    }

    pub fn get_chunk(&mut self, chunk_number: usize) -> Result<&Vec<PacketData>, Box<dyn Error>> {
        if chunk_number >= self.packet_array.len() {
            return Err("Invalid range".into());
        }

        Ok(self.packet_array.get(chunk_number).unwrap())
    }
}
