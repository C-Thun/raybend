//! 缩略图三诊工具：**这一张文件到底能不能出图**。
//!
//! ```bash
//! cargo run -p raybend --example thumb-probe -- <文件路径>
//! cargo run -p raybend --example thumb-probe -- "C:\\src\\tmp\\pic\\P1000019.RW2"
//! ```
//!
//! 它走的是**应用同一条路径**（`thumbnail::render_file`），所以能回答「是解码不行，
//! 还是缓存/界面的问题」：
//!
//! | 输出 | 含义 |
//! | --- | --- |
//! | `✅ 出图：… 占位图=false` | 解码与编码都正常 —— 界面还没出图就去查缓存与前端 |
//! | `⚠️ 返回 None（调用方会改用占位图）` | 解码失败（相机不支持 / 文件损坏 / worker 拉不起来），上面那行 stderr 会写原因 |
//! | `❌ 出错：…` | 更外层的错（文件读不了、编码失败） |
//!
//! 三条实测经验（2026-09-17，排查「RAW 在预览列表里没有画面」时攒下的）：
//!
//! 1. **别给 Windows 的 worker 喂 WSL 路径** —— `RAYBEND_RAW_WORKER=…exe` 时传
//!    `/mnt/c/...` 会得到「拒绝访问 (os error 5)」，那是路径形态不对，不是解码器的问题；
//!    要传 `C:\...`。
//! 2. **让它驱动 Windows 那个 worker 是可行的**（`RAYBEND_RAW_WORKER` 指向 exe）——
//!    在 WSL 里就能验「Windows 产物里的解码器好不好使」。
//! 3. 如果同一张图重新导入后仍然出的是**占位图**，先怀疑**缓存签名没抬版本**
//!    （`render.rs` 的 `PIPELINE_VERSION`，那段注释里记着那次事故）。
//!
//! `RAYBEND_RAW_WORKER=<路径>` 可以指定用哪个 worker 可执行文件（见 `raw::worker`）。

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("用法：thumb-probe <文件路径>");
        std::process::exit(2);
    };
    let path = std::path::Path::new(&arg);
    // 注意：这张存在性检查在 WSL 里判不了 `C:\...` 形式的路径（会显示 false）——
    // 那不代表文件不存在，真伪以 `render_file` 的结果为准。
    println!("文件：{arg}（本进程看得见：{}）", path.exists());

    match raybend::thumbnail::render_file(path, raybend::thumbnail::SizeClass::Grid) {
        Ok(Some(thumb)) => println!(
            "✅ 出图：{}×{}，{} 字节，占位图={}",
            thumb.width,
            thumb.height,
            thumb.data.len(),
            thumb.placeholder
        ),
        Ok(None) => println!("⚠️ 返回 None（调用方会改用占位图）"),
        Err(error) => println!("❌ 出错：{error}"),
    }
}
