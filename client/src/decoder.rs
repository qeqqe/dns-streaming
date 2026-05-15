use std::error::Error;

use ffmpeg_next::{Packet, codec, decoder::Video, frame};

use crate::dns_client::ChunkData;

pub struct Decoder {
    pub decoder: Video,
}

impl Decoder {
    pub fn init() -> Self {
        let codec = codec::decoder::find(codec::Id::HEVC).unwrap();
        let mut ctx = codec::context::Context::new_with_codec(codec);

        unsafe {
            let raw = ctx.as_mut_ptr();
            (*raw).width = 1920;
            (*raw).height = 1080;
            (*raw).pix_fmt = ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_YUV420P;
        }
        let decoder = ctx.decoder().video().unwrap();
        Self { decoder }
    }

    pub fn decode(&mut self, chunk_data: &ChunkData) -> Result<Vec<frame::Video>, Box<dyn Error>> {
        let mut frames = Vec::new();
        let mut decoded_frame = frame::Video::empty();
        for packets in chunk_data.packet_bytes.iter() {
            let pkt = Packet::copy(&packets.pkt_data);
            self.decoder.send_packet(&pkt)?;
            loop {
                match self.decoder.receive_frame(&mut decoded_frame) {
                    Ok(()) => frames.push(decoded_frame.clone()),
                    Err(ffmpeg_next::Error::Other {
                        errno: ffmpeg_next::error::EAGAIN,
                    }) => break,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(frames)
    }
}
