//! 更新包下载与校验：Range 断点续传 + minisign 验签。
//!
//! 为什么不用官方插件的 `Update::download()`：它一次性把整包收进内存（回调只给字节数、
//! 不给数据），既落不了盘也续不了传；签名校验又是它的私有函数。所以 v2 自己下载，
//! 校验语义**照抄插件**（`tauri-plugin-updater` 2.12 的 `verify_signature`）：
//! base64 解码 → `PublicKey::decode` / `Signature::decode` → 验签 → 比对签名里的版本号。
use base64::Engine;
use futures_util::StreamExt;
use minisign_verify::{PublicKey, Signature};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::Duration;
use tauri::Url;

/// 两个 chunk 之间最长允许的静默时间：连接卡死时不至于把「下载中」永远挂在那儿。
const STALL_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("raybend/", env!("CARGO_PKG_VERSION"));
const VERIFY_CHUNK: usize = 256 * 1024;

/// 一次下载要用到的全部远端事实（都来自本次检查的签名清单）。
pub struct Plan {
    pub version: String,
    pub url: Url,
    pub signature: String,
    pub public_key: String,
}

/// 服务器这次回应该怎么落到 `.part` 上。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// 接着写，`from` 是文件里已有的字节数（0 = 空文件）
    Append { from: u64, total: Option<u64> },
    /// 从头写（服务器不认 Range，或本地进度越界）
    Restart { total: Option<u64> },
}

/// 解析 `Content-Range: bytes <start>-<end>/<total>`，取起点与总长（`*` 视作未知）。
pub fn parse_content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let start = range.split_once('-')?.0.trim().parse().ok()?;
    let total = total.trim();
    let total = if total == "*" {
        None
    } else {
        Some(total.parse().ok()?)
    };
    Some((start, total))
}

/// 纯判定：本地已有 `existing` 字节、服务器给了 `status`，这一趟是续写还是重来。
pub fn resume_plan(
    existing: u64,
    status: u16,
    content_range: Option<&str>,
    content_length: Option<u64>,
) -> Result<Resume, String> {
    if status == 416 {
        // 本地那份跟远端对不上（或已越界）：删掉重来
        return Ok(Resume::Restart { total: None });
    }
    if !(200..300).contains(&status) {
        return Err(format!("UPDATE_DOWNLOAD_HTTP:{status}"));
    }
    let ranged = content_range.and_then(parse_content_range);
    let total = match (ranged, content_length) {
        (Some((_, total)), _) if total.is_some() => total,
        (_, length) => length,
    };
    if existing == 0 {
        return Ok(Resume::Append { from: 0, total });
    }
    match status {
        206 => match ranged {
            Some((start, _)) if start == existing => Ok(Resume::Append { from: existing, total }),
            // 起点对不上：这份文件不是我们以为的那份
            _ => Ok(Resume::Restart { total }),
        },
        // 200 = 服务器忽略了 Range，只会给整包
        _ => Ok(Resume::Restart { total }),
    }
}

fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // 与插件 / tauri core 同样的做法：没装过就补一个 ring provider。
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
    });
}

fn client() -> Result<reqwest::Client, String> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|error| format!("UPDATE_DOWNLOAD_CLIENT:{error}"))
}

/// 下载（或续传）到 `part`，返回最终字节数。`on_progress` 每次拿到 `(已下载, 总长)`。
pub async fn fetch<F: FnMut(u64, Option<u64>)>(
    plan: &Plan,
    part: &Path,
    existing: u64,
    mut on_progress: F,
) -> Result<u64, String> {
    let mut request = client()?.get(plan.url.clone());
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("UPDATE_DOWNLOAD_NETWORK:{error}"))?;
    let status = response.status().as_u16();
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let (mut done, total, append) =
        match resume_plan(existing, status, content_range.as_deref(), content_length)? {
            Resume::Append { from, total } => (from, total, true),
            Resume::Restart { total } => (0, total, false),
        };
    let file = if append {
        fs::OpenOptions::new().create(true).append(true).open(part)
    } else {
        fs::File::create(part)
    }
    .map_err(|error| format!("UPDATE_DOWNLOAD_WRITE:{error}"))?;
    let mut writer = BufWriter::new(file);
    let mut stream = response.bytes_stream();
    loop {
        match tokio::time::timeout(STALL_TIMEOUT, stream.next()).await {
            Err(_) => return Err("UPDATE_DOWNLOAD_STALLED".into()),
            Ok(None) => break,
            Ok(Some(Err(error))) => return Err(format!("UPDATE_DOWNLOAD_NETWORK:{error}")),
            Ok(Some(Ok(chunk))) => {
                writer
                    .write_all(&chunk)
                    .map_err(|error| format!("UPDATE_DOWNLOAD_WRITE:{error}"))?;
                done += chunk.len() as u64;
                on_progress(done, total);
            }
        }
    }
    writer
        .flush()
        .map_err(|error| format!("UPDATE_DOWNLOAD_WRITE:{error}"))?;
    if let Some(total) = total {
        if done != total {
            return Err(format!("UPDATE_DOWNLOAD_INCOMPLETE:{done}/{total}"));
        }
    }
    Ok(done)
}

/// 与插件一致：清单里的公钥 / 签名是**文本再 base64** 过的。
fn base64_to_string(value: &str) -> Result<String, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
    String::from_utf8(decoded).map_err(|_| "UPDATE_SIGNATURE_INVALID".into())
}

pub fn decode_public_key(public_key_b64: &str) -> Result<PublicKey, String> {
    PublicKey::decode(&base64_to_string(public_key_b64)?)
        .map_err(|_| "UPDATE_PUBLIC_KEY_INVALID".into())
}

pub fn decode_signature(signature_b64: &str) -> Result<Signature, String> {
    Signature::decode(&base64_to_string(signature_b64)?)
        .map_err(|_| "UPDATE_SIGNATURE_INVALID".into())
}

/// 签名里自带的版本号（`trusted comment` 的 `version:` 字段）与清单版本比对。
///
/// 说明：tauri-cli 2.11.4 产出的签名里**没有**这个字段（实测 v0.1.2 的 `.sig`），
/// 所以这条与插件默认（`require_signed_version = false`）一样是「有则必核、无则放行」。
fn verify_signed_version(trusted_comment: &str, announced: &str) -> Result<(), String> {
    let Some(signed) = trusted_comment
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
    else {
        return Ok(());
    };
    let matches = match (
        semver::Version::parse(signed.trim_start_matches('v')),
        semver::Version::parse(announced.trim_start_matches('v')),
    ) {
        (Ok(signed), Ok(announced)) => signed == announced,
        _ => signed == announced,
    };
    if matches {
        Ok(())
    } else {
        Err(format!("UPDATE_SIGNATURE_VERSION:{signed}!={announced}"))
    }
}

/// 校验落盘的成品：流式哈希（不把 120 MB 读进内存），签名不合法直接拒。
pub fn verify_file(path: &Path, plan: &Plan) -> Result<(), String> {
    let public_key = decode_public_key(&plan.public_key)?;
    let signature = decode_signature(&plan.signature)?;
    match public_key.verify_stream(&signature) {
        Ok(mut verifier) => {
            let file = fs::File::open(path).map_err(|error| format!("UPDATE_DOWNLOAD_READ:{error}"))?;
            let mut reader = BufReader::new(file);
            let mut buffer = vec![0u8; VERIFY_CHUNK];
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| format!("UPDATE_DOWNLOAD_READ:{error}"))?;
                if read == 0 {
                    break;
                }
                verifier.update(&buffer[..read]);
            }
            verifier
                .finalize()
                .map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
        }
        // 老式（非预哈希）签名只能整读一遍 —— 与插件 `verify(data, sig, true)` 对齐
        Err(minisign_verify::Error::UnsupportedLegacyMode) => {
            let bytes = fs::read(path).map_err(|error| format!("UPDATE_DOWNLOAD_READ:{error}"))?;
            public_key
                .verify(&bytes, &signature, true)
                .map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
        }
        Err(_) => return Err("UPDATE_SIGNATURE_INVALID".into()),
    }
    verify_signed_version(signature.trusted_comment(), &plan.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v0.1.2 发布时的真实公钥（`release.config.json`）——公开信息，用来钉住解码路径。
    const REAL_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDgyMUIzRkFEQjRBODNGOTcKUldTWFA2aTByVDhiZ3BXTVlFakNaYWo5YjZpTVdvUTJpYWNzcDkxVkFjekhpczF5M2U1aUVUNjUK";
    /// v0.1.2 那个 120 MB MSI 的真实签名（`latest.json` 的 `signature`）。
    const REAL_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTWFA2aTByVDhiZ3B6ZmYwMG9mb1ZLWnFKMnhiNUVreVlDaXVvSkFBcGo1RXd1a3hGTURhNnNtcjhabGhoWU5PZEx2Q3dzU05VYWluVWRmejRqZkphaTNkb2RtN0xFVUFRPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxOTk5NzYyCWZpbGU6UmF5QmVuZF8wLjEuMl94NjRfZW4tVVMubXNpCjdUbFJxY1h1QzJ4UlhqaG1Zbms4eTVFbXVaZjVEaDVyeUhvTW0ydjludXFId0xCSzRpYWwzVGtQc1JONktZRGtDbGVHSnVVNHo2K1NJQmtUdC9xQUJBPT0K";

    fn plan() -> Plan {
        Plan {
            version: "0.1.2".into(),
            url: Url::parse("https://example.org/a.msi").expect("url"),
            signature: REAL_SIGNATURE.into(),
            public_key: REAL_PUBLIC_KEY.into(),
        }
    }

    #[test]
    fn content_range_parsing() {
        assert_eq!(parse_content_range("bytes 100-999/1000"), Some((100, Some(1000))));
        assert_eq!(parse_content_range("bytes 0-0/1"), Some((0, Some(1))));
        assert_eq!(parse_content_range("bytes 1024-/*"), Some((1024, None)));
        assert_eq!(parse_content_range("items 1-2/3"), None);
        assert_eq!(parse_content_range("bytes abc/3"), None);
    }

    #[test]
    fn resume_decisions() {
        // 空文件：无论 200/206 都是从头写
        assert_eq!(
            resume_plan(0, 200, None, Some(100)).unwrap(),
            Resume::Append { from: 0, total: Some(100) }
        );
        assert_eq!(
            resume_plan(0, 206, Some("bytes 0-99/100"), Some(100)).unwrap(),
            Resume::Append { from: 0, total: Some(100) }
        );
        // 有进度：206 且起点对得上 ⇒ 续写（总长从 Content-Range 取）
        assert_eq!(
            resume_plan(50, 206, Some("bytes 50-99/100"), Some(50)).unwrap(),
            Resume::Append { from: 50, total: Some(100) }
        );
        // 起点对不上 ⇒ 重来
        assert_eq!(
            resume_plan(50, 206, Some("bytes 0-99/100"), Some(50)).unwrap(),
            Resume::Restart { total: Some(100) }
        );
        // 服务器忽略 Range（200）⇒ 重来
        assert_eq!(
            resume_plan(50, 200, None, Some(100)).unwrap(),
            Resume::Restart { total: Some(100) }
        );
        // 416 ⇒ 本地那份没用了，删掉重来
        assert_eq!(
            resume_plan(50, 416, None, None).unwrap(),
            Resume::Restart { total: None }
        );
        // 其它错误码 ⇒ 直接失败
        assert!(resume_plan(0, 503, None, None).unwrap_err().contains("503"));
    }

    #[test]
    fn real_material_decodes_but_bad_input_does_not() {
        assert!(decode_public_key(REAL_PUBLIC_KEY).is_ok());
        assert!(decode_signature(REAL_SIGNATURE).is_ok());
        assert!(decode_public_key("not base64!!").is_err());
        assert!(decode_public_key("").is_err());
        assert!(decode_signature("bm90IGEgc2ln").is_err(), "解出来不是签名文件");
    }

    #[test]
    fn signature_does_not_match_other_content() {
        // 真实签名 + 别的内容 ⇒ 必须拒（证明校验真的在跑，不是只解了个码）
        let dir = std::env::temp_dir().join(format!("raybend-verify-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("fake.bin");
        fs::write(&file, b"this is not the signed installer").expect("写文件");
        let error = verify_file(&file, &plan()).unwrap_err();
        assert_eq!(error, "UPDATE_SIGNATURE_INVALID");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn signed_version_check_is_optional_but_enforced_when_present() {
        // 我们自己的签名没有 version: 字段（实测）⇒ 放行
        assert!(verify_signed_version("timestamp:1\tfile:a.msi", "0.1.2").is_ok());
        // 有则必须一致（`v` 前缀按 semver 归一）
        assert!(verify_signed_version("version:0.1.2", "0.1.2").is_ok());
        assert!(verify_signed_version("version:v0.1.2", "0.1.2").is_ok());
        assert!(verify_signed_version("version:0.1.1", "0.1.2").is_err());
        // 非 semver 的字面比对
        assert!(verify_signed_version("version:1.0.0-rc", "1.0.0-rc").is_ok());
        assert!(verify_signed_version("version:1.0.0-rc", "1.0.0").is_err());
    }
}
