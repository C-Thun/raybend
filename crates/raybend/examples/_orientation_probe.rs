//! 临时调试：打印文件 EXIF 方向与 RAW 解码器报的方向（用完删除）。
use std::path::Path;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let p = Path::new(&path);
    let data = raybend::media::exif::read_file_raw(p);
    println!("file orientation = {:?}", data.orientation);
    let request = raybend::raw::DecodeRequest::thumb(p.to_path_buf(), 1024).with_preview(false);
    let decoded = raybend::raw::worker::shared()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .decode_linear(&request)
        .expect("decode");
    println!("decoder orientation = {:?}, size = {}x{}", decoded.orientation, decoded.width, decoded.height);
    println!("raw_orientation(file, decoder) = {:?}", raybend::media::exif::raw_orientation(p, decoded.orientation));
}
