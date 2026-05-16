#![allow(dead_code, unused_variables, unused_mut)]

use crate::{decoder::Decoder, dns_client::DNSClient};

use ffmpeg_next::{
    frame,
    software::scaling::{context::Context, flag::Flags},
};

mod decoder;
mod dns_client;

#[tokio::main]
async fn main() {
    let mut client = DNSClient::get_client("127.0.0.1:5300".to_string()).await;

    let mut decoder = Decoder::init();

    let mut chunk_index = 0;
    let mut window: Option<minifb::Window> = None;
    let mut scaler: Option<Context> = None;
    let mut rgb_frame = frame::Video::empty();

    loop {
        let chunk = match client.request_chunk(chunk_index).await {
            Ok(c) => c,
            Err(_) => {
                println!(
                    "Failed to get chunk {} or timeout. Continuing to next chunk.",
                    chunk_index
                );
                chunk_index += 1;
                continue;
            }
        };

        let frames = decoder.decode(&chunk).unwrap();

        for yuv_frame in &frames {
            if scaler.is_none() {
                scaler = Some(
                    Context::get(
                        yuv_frame.format(),
                        yuv_frame.width(),
                        yuv_frame.height(),
                        ffmpeg_next::format::Pixel::BGRA,
                        yuv_frame.width(),
                        yuv_frame.height(),
                        Flags::BILINEAR,
                    )
                    .unwrap(),
                );
            }

            if window.is_none() {
                let mut new_window = minifb::Window::new(
                    "Video Player",
                    yuv_frame.width() as usize,
                    yuv_frame.height() as usize,
                    minifb::WindowOptions::default(),
                )
                .unwrap();
                // 24 FPS so teh loop doesnt request data as fast as possibile
                new_window.set_target_fps(24);
                window = Some(new_window);
            }

            let scaler_ctx = scaler.as_mut().unwrap();
            scaler_ctx.run(yuv_frame, &mut rgb_frame).unwrap();

            let width = rgb_frame.width() as usize;
            let height = rgb_frame.height() as usize;
            let stride = rgb_frame.stride(0);
            let data = rgb_frame.data(0);

            // copy BGRA bytes to u32 buffer for minifb
            let mut buffer = vec![0u32; width * height];
            for y in 0..height {
                let row_start = y * stride;
                let row_end = row_start + width * 4;
                let row_data = &data[row_start..row_end];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        row_data.as_ptr(),
                        buffer[y * width..].as_mut_ptr() as *mut u8,
                        width * 4,
                    );
                }
            }

            if let Some(win) = window.as_mut() {
                if !win.is_open() || win.is_key_down(minifb::Key::Escape) {
                    return;
                }
                win.update_with_buffer(&buffer, width, height).unwrap();
            }
        }

        chunk_index += 1;
    }
}
