//! 直方图取数探针（零依赖，用量少但**定位事故时很值**）。
//!
//! ```bash
//! cargo run -p raybend --example hist-probe -- /mnt/c/src/tmp/pic/P1000019.RW2 /mnt/c/src/tmp/pic/P1000001.JPG
//! ```
//!
//! **为什么留着它**：2026-09-24 缓存图格式换成 AVIF 之后，`histogram_of_file` 用
//! `image::load_from_memory` 读**我们自己渲染出来的字节** —— 而 `image` 的 avif feature
//! **只有编码器、没有解码器**，于是直方图恒为空（界面表现：编辑右栏直方图空态、
//! 曲线背景的直方图底纹不出现）。当时就是用它把「空态」钉成 `Ok(None)` 的，
//! 修好之后它会打出 ✓ —— 换格式、改取数路径时都该跑一遍。

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("用法：hist-probe <照片路径>…（RAW 与位图都可以）");
        std::process::exit(2);
    }
    let mut bad = 0;
    for path in &paths {
        match raybend::display::histogram_of_file(std::path::Path::new(path), 86) {
            Ok(Some(h)) => println!(
                "✓ {path}\n    86 桶，峰值 {:.0}，R 桶计数合计 {:.0}",
                h.max,
                h.r.iter().sum::<f64>()
            ),
            Ok(None) => {
                bad += 1;
                println!("✗ {path}\n    **拿不到直方图**（`Ok(None)`）—— 界面上就是空态");
            }
            Err(error) => {
                bad += 1;
                println!("! {path}\n    {error}");
            }
        }
    }
    std::process::exit(i32::from(bad > 0));
}
