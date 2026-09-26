//! Single-way external edit: bounded platform discovery and shared, cancellable TIFF output.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
#[cfg(windows)]
mod windows;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Application {
    pub name: String,
    pub path: String,
}
pub struct Candidate {
    pub name: &'static str,
    pub exes: &'static [&'static str],
    pub paths: &'static [&'static str],
}
pub const CANDIDATES: &[Candidate] = &[
    Candidate {
        name: "Adobe Photoshop",
        exes: &["Photoshop.exe"],
        paths: &["Adobe/Adobe Photoshop*/Photoshop.exe"],
    },
    Candidate {
        name: "Affinity Photo",
        exes: &["Photo.exe", "AffinityPhoto2.exe", "AffinityPhoto.exe"],
        paths: &[
            "Affinity/Photo*/Photo.exe",
            "WindowsApps/affinityphoto2.exe",
        ],
    },
    Candidate {
        name: "Affinity",
        exes: &["Affinity.exe"],
        paths: &[
            "Affinity/Affinity*/Affinity.exe",
            "Canva/Affinity*/Affinity.exe",
            "WindowsApps/Affinity.exe",
        ],
    },
    Candidate {
        name: "GIMP",
        exes: &["gimp-3.exe", "gimp-3.0.exe", "gimp-2.10.exe"],
        paths: &[
            "GIMP*/bin/gimp-3.exe",
            "GIMP*/bin/gimp-3.0.exe",
            "GIMP*/bin/gimp-2.10.exe",
        ],
    },
    Candidate {
        name: "Krita",
        exes: &["krita.exe"],
        paths: &["Krita*/bin/krita.exe"],
    },
    Candidate {
        name: "PaintShop Pro",
        exes: &["Corel PaintShop Pro.exe"],
        paths: &[
            "Corel/Corel PaintShop Pro*/Corel PaintShop Pro.exe",
            "Corel/Corel PaintShop Pro*/64-bit/Corel PaintShop Pro.exe",
        ],
    },
    Candidate {
        name: "Topaz Photo AI",
        exes: &["Topaz Photo AI.exe"],
        paths: &["Topaz Labs LLC/Topaz Photo AI/Topaz Photo AI.exe"],
    },
    Candidate {
        name: "RawTherapee",
        exes: &["rawtherapee.exe"],
        paths: &["RawTherapee*/rawtherapee.exe"],
    },
    Candidate {
        name: "RapidRAW",
        exes: &["RapidRAW.exe", "rapidraw.exe"],
        paths: &["RapidRAW*/RapidRAW.exe", "RapidRAW*/rapidraw.exe"],
    },
];
/// Platform boundary: tests replace filesystem/registry/launch, never start a real editor.
pub trait Platform {
    fn discover(&self) -> Vec<Application>;
    fn available(&self, path: &Path) -> bool;
    fn launch(&self, path: &Path, file: &Path) -> Result<()>;
}
pub fn path_key(path: &str) -> String {
    let path = path.trim().trim_matches('"');
    let drive = path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && path.as_bytes().get(1) == Some(&b':')
        && path
            .as_bytes()
            .get(2)
            .is_some_and(|c| matches!(c, b'/' | b'\\'));
    if drive || path.starts_with("\\\\") || path.starts_with("//") {
        path.replace('\\', "/").to_lowercase()
    } else {
        path.to_owned()
    }
}
pub fn checked(platform: &impl Platform, applications: &[Application]) -> Vec<Application> {
    let mut seen = std::collections::HashSet::new();
    applications
        .iter()
        .filter(|app| {
            !app.name.trim().is_empty()
                && app.name.chars().count() <= 120
                && Path::new(&app.path).is_absolute()
                && !app.path.contains('\0')
                && platform.available(Path::new(&app.path))
                && seen.insert(path_key(&app.path))
        })
        .cloned()
        .collect()
}
pub fn discover(platform: &impl Platform) -> Vec<Application> {
    checked(platform, &platform.discover())
}

pub struct Native;
impl Platform for Native {
    fn discover(&self) -> Vec<Application> {
        #[cfg(windows)]
        {
            windows::discover()
        }
        #[cfg(not(windows))]
        {
            vec![]
        }
    }
    fn available(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn launch(&self, path: &Path, file: &Path) -> Result<()> {
        if !path.is_absolute() || !file.is_absolute() || !file.is_file() {
            return Err(Error::Unsupported(
                "外部应用和 TIFF 需要有效的绝对路径".into(),
            ));
        }
        #[cfg(windows)]
        {
            windows::launch(path, file)
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new(path).arg(file).spawn()?;
            Ok(())
        }
    }
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(Error::Unsupported("EXTERNAL_CANCELLED".into()))
    } else {
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn write_tiff(
    source: &Path,
    captured: &crate::export::VariantSnapshot,
    directory: &Path,
    metadata: &crate::export::metadata::Metadata,
    lens: Option<&crate::develop::lens::LensCorrection>,
    lut: Option<&crate::develop::lut::Lut>,
    cancel: &AtomicBool,
    phase: impl Fn(&str),
) -> Result<PathBuf> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(Error::Unsupported("TIFF 保存目录不可用".into()));
    }
    cancelled(cancel)?;
    phase("rendering");
    let image = crate::export::render_captured(source, captured, lens, lut, 0)?;
    cancelled(cancel)?;
    phase("writing");
    let bytes = crate::export::output::encode(&image, "tiff", 90, metadata)?;
    cancelled(cancel)?;
    crate::export::check_source(source, captured)?;
    // FILENAME expansion uses the same filename sanitization as ordinary exports.
    let preset = crate::export::Preset {
        id: "external".into(),
        name: "external".into(),
        format: "tiff".into(),
        quality: 90,
        max_edge: 0,
        size_mode: crate::export::SizeMode::Original,
        percent: 100,
        directory: directory.to_string_lossy().into_owned(),
        template: ":FILENAME".into(),
        existing_file: crate::export::ExistingFile::Append,
    };
    let relative = crate::export::output::relative_name(&preset, source, &Default::default(), 1)?;
    crate::export::output::publish(directory, &relative, &bytes, source)
}
#[cfg(test)]
mod tests {
    use super::*;
    const APP_PATH: &str = if cfg!(windows) {
        "C:/apps/Photoshop.exe"
    } else {
        "/apps/Photoshop.exe"
    };
    struct Fake;
    impl Platform for Fake {
        fn discover(&self) -> Vec<Application> {
            vec![
                Application {
                    name: "Adobe Photoshop".into(),
                    path: APP_PATH.into(),
                },
                Application {
                    name: "duplicate".into(),
                    path: APP_PATH.into(),
                },
                Application {
                    name: "uninstalled".into(),
                    path: "/gone.exe".into(),
                },
            ]
        }
        fn available(&self, path: &Path) -> bool {
            path == Path::new(APP_PATH)
        }
        fn launch(&self, _: &Path, _: &Path) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn bounded_candidates_deduplicate_and_filter_uninstalled() {
        assert_eq!(CANDIDATES.len(), 9);
        let apps = discover(&Fake);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Adobe Photoshop");
    }
    #[test]
    fn path_normalization_and_invalid_registration() {
        assert_eq!(path_key(" \"C:\\编辑器\\APP.EXE\" "), "c:/编辑器/app.exe");
        assert_ne!(path_key("/Apps/Editor"), path_key("/apps/editor"));
        assert_eq!(path_key("/apps/a\\b"), "/apps/a\\b");
        assert!(
            checked(
                &Fake,
                &[
                    Application {
                        name: " ".into(),
                        path: APP_PATH.into()
                    },
                    Application {
                        name: "x".into(),
                        path: "relative.exe".into()
                    }
                ]
            )
            .is_empty()
        );
    }
    #[test]
    fn cancellation_is_cooperative() {
        let cancel = AtomicBool::new(true);
        assert!(
            cancelled(&cancel)
                .unwrap_err()
                .to_string()
                .contains("EXTERNAL_CANCELLED")
        );
        cancel.store(false, Ordering::Release);
        assert!(cancelled(&cancel).is_ok());
    }
    fn fixture() -> (tempfile::TempDir, PathBuf, crate::export::VariantSnapshot) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        let image = crate::display::output::Rgb16Image::from_fn(8, 6, |x, y| {
            image::Rgb([(x * 1000 + y * 97) as u16, 12345, 54321])
        });
        image.save(&source).unwrap();
        let stack = crate::store::develop::DevelopStack {
            source_base: crate::store::develop::EditBase::Sooc,
            ..Default::default()
        };
        let snap = crate::export::VariantSnapshot {
            reference: crate::export::VariantRef {
                asset_id: 1,
                variant: "sooc".into(),
            },
            name: "SOOC".into(),
            rel_path: "原片.png".into(),
            profile_hash: crate::store::issues::profile_hash(&stack).unwrap(),
            stack,
            source_signature: crate::media::source::source_signature(&source).unwrap(),
        };
        (dir, source, snap)
    }
    #[test]
    fn tiff_shared_precision_metadata_no_overwrite_and_source_hash() {
        use sha2::{Digest, Sha256};
        let (dir, source, snap) = fixture();
        let before = Sha256::digest(std::fs::read(&source).unwrap());
        let cancel = AtomicBool::new(false);
        let md = crate::export::metadata::Metadata {
            author: Some("崔总".into()),
            keywords: vec!["柔光".into()],
            ..Default::default()
        };
        let first =
            write_tiff(&source, &snap, dir.path(), &md, None, None, &cancel, |_| {}).unwrap();
        let second =
            write_tiff(&source, &snap, dir.path(), &md, None, None, &cancel, |_| {}).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            crate::export::render_captured(&source, &snap, None, None, 0).unwrap(),
            image::open(&first).unwrap().to_rgb16()
        );
        assert_eq!(
            image::open(&first).unwrap().color(),
            image::ColorType::Rgb16
        );
        assert!(
            image::open(&first)
                .unwrap()
                .to_rgb16()
                .as_raw()
                .iter()
                .any(|value| value % 257 != 0)
        );
        let bytes = std::fs::read(first).unwrap();
        assert!(bytes.windows("柔光".len()).any(|w| w == "柔光".as_bytes()));
        assert_eq!(before, Sha256::digest(std::fs::read(&source).unwrap()));
    }
    #[test]
    fn cancellation_before_and_after_render_never_publishes() {
        let (dir, source, snap) = fixture();
        let md = Default::default();
        let cancel = AtomicBool::new(true);
        assert!(write_tiff(&source, &snap, dir.path(), &md, None, None, &cancel, |_| {}).is_err());
        cancel.store(false, Ordering::Release);
        assert!(
            write_tiff(
                &source,
                &snap,
                dir.path(),
                &md,
                None,
                None,
                &cancel,
                |phase| {
                    if phase == "writing" {
                        cancel.store(true, Ordering::Release)
                    }
                }
            )
            .is_err()
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn unavailable_destination_or_replaced_source_leaves_no_output() {
        let (dir, source, snap) = fixture();
        let cancel = AtomicBool::new(false);
        let md = Default::default();
        assert!(
            write_tiff(
                &source,
                &snap,
                &dir.path().join("missing"),
                &md,
                None,
                None,
                &cancel,
                |_| {}
            )
            .is_err()
        );
        std::fs::write(&source, b"replaced").unwrap();
        assert!(write_tiff(&source, &snap, dir.path(), &md, None, None, &cancel, |_| {}).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
