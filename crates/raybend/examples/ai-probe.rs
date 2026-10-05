//! 实验入口：不编入默认应用，不把未校准分数写成标签；与隔离 worker 共用唯一编码器。
use raybend::ai::{
    preprocess::{self, ResizePolicy},
    runtime::Encoder,
    scoring::{self, Concept},
};
use std::{
    error::Error,
    path::{Path, PathBuf},
    time::Instant,
};
fn floats(path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
    let data = std::fs::read(path)?;
    if data.len() % 4 != 0 {
        return Err("tensor file must be little-endian f32".into());
    }
    Ok(data
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}
fn main() -> Result<(), Box<dyn Error>> {
    if std::env::args().any(|a| a == raybend::ai::worker::WORKER_ARG) {
        std::process::exit(raybend::ai::worker::run_worker_main());
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 {
        return Err(
            "usage: ai-probe <absolute CPU ORT library> <probe output directory> [images]".into(),
        );
    }
    let lib = PathBuf::from(&args[0]);
    let dir = PathBuf::from(&args[1]);
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("probe.json"))?)?;
    let siglip = config["model_id"]
        .as_str()
        .is_some_and(|id| id.starts_with("google/siglip2"));
    let start = Instant::now();
    let mut encoder = Encoder::load_sized(
        &lib,
        &dir.join("image_encoder.onnx"),
        if siglip { 768 } else { 512 },
    )?;
    let load = start.elapsed().as_secs_f64();
    let input = floats(&dir.join("parity-input.f32"))?;
    let expected = floats(&dir.join("parity-expected.f32"))?;
    let mut timings = Vec::new();
    let mut output = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        output = encoder.encode(input.clone())?;
        timings.push(start.elapsed().as_secs_f64());
    }
    if output.len() != expected.len() {
        return Err("reference shape differs".into());
    }
    let max_error = output
        .iter()
        .zip(&expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0_f32, f32::max);
    if max_error > 0.0001 {
        return Err(format!("ONNX reference mismatch: {max_error}").into());
    }
    timings.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"library":lib,"provider":"CPUExecutionProvider","load_s":load,"load_includes_contract_probe":true,"p50_s":timings[15],"p95_s":timings[28],"max_abs_error":max_error,"samples":30,"quality_validated":false})
    );
    let concepts: Vec<Concept> = serde_json::from_value(config["concepts"].clone())?;
    if args.get(2).is_some_and(|arg| arg == "--score-list") {
        let list = args.get(3).ok_or("missing score-list manifest")?;
        let samples: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(list)?)?;
        if samples.len() > 5000 {
            return Err("score-list limit is 5000".into());
        }
        for sample in samples {
            let path = PathBuf::from(sample["path"].as_str().ok_or("missing image path")?);
            let result = (|| -> Result<serde_json::Value, Box<dyn Error>> {
                let rgb = preprocess::original_path(&path, false)?;
                let mut results = std::collections::BTreeMap::new();
                for policy in [ResizePolicy::CenterCrop, ResizePolicy::Fit] {
                    let data = encoder.encode(if siglip {
                        preprocess::siglip_tensor(&rgb)?
                    } else {
                        preprocess::tensor(&rgb, policy)?
                    })?;
                    let scores = scoring::score(&data, &concepts)?;
                    results.insert(
                        format!("{policy:?}"),
                        concepts
                            .iter()
                            .zip(scores)
                            .map(|(c, s)| (&c.key, s))
                            .collect::<std::collections::BTreeMap<_, _>>(),
                    );
                }
                Ok(serde_json::to_value(results)?)
            })();
            println!(
                "{}",
                match result {
                    Ok(scores) => serde_json::json!({"id":sample["id"],"scores":scores}),
                    Err(error) => serde_json::json!({"id":sample["id"],"error":error.to_string()}),
                }
            );
        }
        return Ok(());
    }
    let shutdown = raybend::ai::worker::Shutdown::default();
    let mut worker = raybend::ai::worker::Worker::with_shutdown(
        std::env::current_exe()?,
        lib.clone(),
        dir.join("image_encoder.onnx"),
        shutdown.clone(),
    );
    for path in args.iter().skip(2) {
        let path = PathBuf::from(path);
        let rgb = preprocess::original_path(&path, false)?;
        for policy in [ResizePolicy::CenterCrop, ResizePolicy::Fit] {
            let input = preprocess::tensor(&rgb, policy)?;
            std::fs::write(
                path.with_extension(format!("{policy:?}.f32")),
                input
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>(),
            )?;
            let data = encoder.encode(input)?;
            let scores = scoring::score(&data, &concepts)?;
            std::fs::write(
                path.with_extension(format!("{policy:?}.features.f32")),
                data.iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>(),
            )?;
            let isolated = worker.encode(&path, false, policy)?;
            let worker_error = data
                .iter()
                .zip(&isolated)
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            if worker_error > 0.0001 {
                return Err("isolated worker parity mismatch".into());
            }
            println!(
                "{}",
                serde_json::json!({"image":path,"policy":policy,"worker_max_abs_error":worker_error,"scores":concepts.iter().zip(scores).map(|(c,s)|(&c.key,s)).collect::<std::collections::BTreeMap<_,_>>(),"quality_validated":false})
            );
        }
    }
    shutdown.stop();
    if worker.probe().is_ok() {
        return Err("stopped worker accepted a new request".into());
    }
    println!(
        "{}",
        serde_json::json!({"shutdown_probed":true,"stopped":shutdown.stopped()})
    );
    Ok(())
}
