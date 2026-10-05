//! Read-only real RAW numeric regression. Usage: color-raw-probe FILE [edge=2048] [worker]
//! All rawler calls execute in the existing isolated worker. No image is written.
use raybend::raw::{DecodeRequest, worker::RawWorker};
use sha2::{Digest, Sha256};
fn fingerprint(image: &raybend::raw::backend::RawWorkingImage) -> (String, [f32; 2], usize, usize) {
    let mut hash = Sha256::new();
    let mut bounds = [f32::INFINITY, f32::NEG_INFINITY];
    let (mut negative, mut high) = (0, 0);
    for value in image.image.pixels().iter().flatten() {
        hash.update(value.to_le_bytes());
        bounds[0] = bounds[0].min(*value);
        bounds[1] = bounds[1].max(*value);
        negative += usize::from(*value < 0.0);
        high += usize::from(*value > 1.0);
    }
    (format!("{:x}", hash.finalize()), bounds, negative, high)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or("missing RAW path")?;
    let edge = args
        .get(2)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(2048_u32);
    let mut worker = args.get(3).map_or_else(RawWorker::new, RawWorker::with_bin);
    let base = DecodeRequest::full(path).with_preview(false);
    let mut request = base.clone();
    request.max_edge = (edge > 0).then_some(edge);
    let mut identity_request = base;
    identity_request.max_edge = Some(1);
    let started = std::time::Instant::now();
    let probed_id = worker.working_color_identity(std::path::Path::new(path))?;
    println!("metadata identity incl. worker start {:?}", started.elapsed());
    let started = std::time::Instant::now();
    assert_eq!(worker.working_color_identity(std::path::Path::new(path))?, probed_id);
    println!("metadata identity warm worker {:?}", started.elapsed());
    let legacy_id = worker.decode_linear(&identity_request)?.camera_matrix_id;
    let started = std::time::Instant::now();
    let current = worker.decode_working(&request)?;
    let current_id = current.camera_matrix_id.clone();
    assert_eq!(current_id, probed_id);
    let current_summary = fingerprint(&current);
    println!(
        "current {:?} {:?}; id={}; summary={current_summary:?}",
        current.image.dimensions(),
        started.elapsed(),
        current_id.as_str()
    );
    drop(current);
    assert_ne!(legacy_id, current_id);
    let started = std::time::Instant::now();
    let old = worker.decode_working(&request.clone().with_camera_matrix(legacy_id.clone()))?;
    let old_summary = fingerprint(&old);
    assert_eq!(old.camera_matrix_id, legacy_id);
    println!(
        "legacy {:?} {:?}; id={}; summary={old_summary:?}",
        old.image.dimensions(),
        started.elapsed(),
        legacy_id.as_str()
    );
    drop(old);
    let replay = worker.decode_working(&request.with_camera_matrix(current_id))?;
    assert_eq!(fingerprint(&replay), current_summary);
    println!(
        "frozen current replay exact; legacy path selected independently; visual quality unverified"
    );
    Ok(())
}
