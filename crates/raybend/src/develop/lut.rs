//! `.cube` / PNG、TIFF HaldCLUT 的同一套三线性求值器。

use crate::error::{Error, Result};
use std::path::Path;

pub const SAMPLE_WEBP: &[u8] = include_bytes!("../../assets/lut-sample.webp");
pub const SAMPLE_WIDTH: u32 = 768;
pub const SAMPLE_HEIGHT: u32 = 576;

#[derive(Debug, Clone)]
pub struct Lut {
    domain_min: [f32; 3],
    domain_max: [f32; 3],
    shaper: Vec<[f32; 3]>,
    cube_size: usize,
    cube: Vec<[f32; 3]>,
}

fn error(message: impl Into<String>) -> Error {
    Error::Unsupported(message.into())
}

impl Lut {
    pub fn load(path: &Path) -> Result<Self> {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("cube") => Self::parse_cube(&std::fs::read_to_string(path)?),
            Some("png" | "tif" | "tiff") => Self::from_hald(path),
            _ => Err(error("只支持 .cube、PNG/TIFF HaldCLUT")),
        }
    }

    pub fn parse_cube(input: &str) -> Result<Self> {
        let mut domain_min = [0.0; 3];
        let mut domain_max = [1.0; 3];
        let mut shaper_size = 0;
        let mut cube_size = 0;
        let mut values = Vec::new();
        let mut data_started = false;
        for (line_index, raw) in input.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields[0].to_ascii_uppercase().as_str() {
                "TITLE" if !data_started => {}
                "DOMAIN_MIN" | "DOMAIN_MAX" if !data_started => {
                    if fields.len() != 4 {
                        return Err(error(format!(
                            "LUT 第 {} 行输入域必须有 3 个值",
                            line_index + 1
                        )));
                    }
                    let mut triplet = [0.0; 3];
                    for channel in 0..3 {
                        triplet[channel] = fields[channel + 1]
                            .parse::<f32>()
                            .map_err(|_| error("LUT 输入域存在非法数字"))?;
                    }
                    if fields[0].eq_ignore_ascii_case("DOMAIN_MIN") {
                        domain_min = triplet;
                    } else {
                        domain_max = triplet;
                    }
                }
                "LUT_1D_SIZE" | "LUT_3D_SIZE"
                    if !data_started
                        || (fields[0].eq_ignore_ascii_case("LUT_3D_SIZE")
                            && cube_size == 0
                            && shaper_size > 0
                            && values.len() == shaper_size) =>
                {
                    if fields.len() != 2 {
                        return Err(error("LUT 尺寸格式无效"));
                    }
                    let size = fields[1]
                        .parse::<usize>()
                        .map_err(|_| error("LUT 尺寸无效"))?;
                    if fields[0].eq_ignore_ascii_case("LUT_1D_SIZE") {
                        if !(2..=65536).contains(&size) || shaper_size != 0 {
                            return Err(error("1D LUT 尺寸无效或重复"));
                        }
                        shaper_size = size;
                    } else {
                        if !(2..=65).contains(&size) || cube_size != 0 {
                            return Err(error("3D LUT 尺寸无效或重复"));
                        }
                        cube_size = size;
                    }
                }
                _ => {
                    data_started = true;
                    if fields.len() != 3 {
                        return Err(error(format!(
                            "LUT 第 {} 行不是 RGB 三元组",
                            line_index + 1
                        )));
                    }
                    let mut triplet = [0.0; 3];
                    for channel in 0..3 {
                        triplet[channel] = fields[channel]
                            .parse::<f32>()
                            .map_err(|_| error("LUT 表项存在非法数字"))?;
                        if !triplet[channel].is_finite() {
                            return Err(error("LUT 表项不能是 NaN 或无穷大"));
                        }
                    }
                    values.push(triplet);
                }
            }
        }
        if (0..3).any(|channel| {
            !domain_min[channel].is_finite()
                || !domain_max[channel].is_finite()
                || domain_min[channel] >= domain_max[channel]
        }) {
            return Err(error("LUT 输入域必须是有限且严格递增的值"));
        }
        if shaper_size == 0 && cube_size == 0 {
            return Err(error("LUT 缺少 1D/3D 尺寸声明"));
        }
        let expected = shaper_size + cube_size.pow(3);
        if values.len() != expected {
            return Err(error(format!(
                "LUT 表项数量错误：应为 {expected}，实际 {}",
                values.len()
            )));
        }
        let cube = values.split_off(shaper_size);
        Ok(Self {
            domain_min,
            domain_max,
            shaper: values,
            cube_size,
            cube,
        })
    }

    pub fn from_hald(path: &Path) -> Result<Self> {
        let image = image::open(path)
            .map_err(|err| error(format!("HaldCLUT 图像无法解码：{err}")))?
            .to_rgb16();
        let (width, height) = image.dimensions();
        if width != height {
            return Err(error("HaldCLUT 必须是正方形"));
        }
        let level = (f64::from(width).cbrt().round()) as u32;
        if !(2..=10).contains(&level) || level.pow(3) != width {
            return Err(error("HaldCLUT 边长必须是 level³（level 2–10）"));
        }
        let cube_size = level.pow(2) as usize;
        let cube = image
            .pixels()
            .map(|pixel| pixel.0.map(|value| f32::from(value) / 65535.0))
            .collect();
        Ok(Self {
            domain_min: [0.0; 3],
            domain_max: [1.0; 3],
            shaper: Vec::new(),
            cube_size,
            cube,
        })
    }

    fn interpolate_1d(table: &[[f32; 3]], channel: usize, x: f32) -> f32 {
        let index = x.clamp(0.0, 1.0) * (table.len() - 1) as f32;
        let lo = index.floor() as usize;
        let hi = (lo + 1).min(table.len() - 1);
        let t = index - lo as f32;
        table[lo][channel] * (1.0 - t) + table[hi][channel] * t
    }

    fn cube_at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.cube[r + self.cube_size * g + self.cube_size * self.cube_size * b]
    }

    pub fn eval(&self, input: [f32; 3]) -> [f32; 3] {
        let mut xyz = [0.0; 3];
        for channel in 0..3 {
            xyz[channel] = ((input[channel] - self.domain_min[channel])
                / (self.domain_max[channel] - self.domain_min[channel]))
                .clamp(0.0, 1.0);
            if !self.shaper.is_empty() {
                xyz[channel] =
                    Self::interpolate_1d(&self.shaper, channel, xyz[channel]).clamp(0.0, 1.0);
            }
        }
        if self.cube_size == 0 {
            return xyz;
        }
        let max = self.cube_size - 1;
        let index = xyz.map(|value| value * max as f32);
        let low = index.map(|value| value.floor() as usize);
        let high = low.map(|value| (value + 1).min(max));
        let t = std::array::from_fn::<_, 3, _>(|channel| index[channel] - low[channel] as f32);
        let mut result = [0.0; 3];
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    let weight = [
                        if r == 0 { 1.0 - t[0] } else { t[0] },
                        if g == 0 { 1.0 - t[1] } else { t[1] },
                        if b == 0 { 1.0 - t[2] } else { t[2] },
                    ]
                    .iter()
                    .product::<f32>();
                    let point = self.cube_at(
                        if r == 0 { low[0] } else { high[0] },
                        if g == 0 { low[1] } else { high[1] },
                        if b == 0 { low[2] } else { high[2] },
                    );
                    for channel in 0..3 {
                        result[channel] += point[channel] * weight;
                    }
                }
            }
        }
        result.map(|value| value.clamp(0.0, 1.0))
    }

    pub fn apply_rgb8(&self, rgb: &mut [u8], strength: f32) -> Result<()> {
        if !rgb.len().is_multiple_of(3) || !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
            return Err(error("LUT 输入像素长度或强度无效"));
        }
        if strength == 0.0 {
            return Ok(());
        }
        for pixel in rgb.as_chunks_mut::<3>().0 {
            let original = [pixel[0], pixel[1], pixel[2]].map(|value| f32::from(value) / 255.0);
            let mapped = self.eval(original);
            for channel in 0..3 {
                pixel[channel] = ((original[channel] * (1.0 - strength)
                    + mapped[channel] * strength)
                    .clamp(0.0, 1.0)
                    * 255.0)
                    .round() as u8;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_red_fastest_and_trilinear_midpoint() {
        let mut text = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text.push_str(&format!("{r} {g} {b}\n"));
                }
            }
        }
        let lut = Lut::parse_cube(&text).unwrap();
        assert_eq!(lut.eval([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert_eq!(lut.eval([0.5, 0.5, 0.5]), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn shaper_then_cube_and_domain() {
        let input = "DOMAIN_MIN -1 -1 -1\nDOMAIN_MAX 1 1 1\nLUT_1D_SIZE 2\n0 0 0\n0.5 0.5 0.5\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = Lut::parse_cube(input).unwrap();
        assert_eq!(lut.eval([1.0, 1.0, 1.0]), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn invalid_count_nan_and_input_are_rejected() {
        assert!(Lut::parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
        assert!(Lut::parse_cube("LUT_1D_SIZE 2\nNaN 0 0\n1 1 1\n").is_err());
        let lut = Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap();
        assert!(lut.apply_rgb8(&mut [1, 2], 1.0).is_err());
    }

    #[test]
    #[ignore = "requires the optional C:/src/resource/Free fixture directory"]
    fn user_cube_samples_parse_and_render() {
        let directory = std::env::var_os("RAYBEND_LUT_FIXTURES").expect("set RAYBEND_LUT_FIXTURES");
        let mut count = 0;
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("cube"))
            {
                let lut =
                    Lut::load(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                let mut pixel = [48, 128, 230];
                lut.apply_rgb8(&mut pixel, 1.0).unwrap();
                count += 1;
            }
        }
        assert_eq!(count, 7);
    }

    #[test]
    fn sample_is_clean_webp() {
        let image =
            image::load_from_memory_with_format(SAMPLE_WEBP, image::ImageFormat::WebP).unwrap();
        assert_eq!(
            (image.width(), image.height()),
            (SAMPLE_WIDTH, SAMPLE_HEIGHT)
        );
        assert_eq!(&SAMPLE_WEBP[..4], b"RIFF");
        assert!(!SAMPLE_WEBP.windows(4).any(|window| window == b"EXIF"));
    }
}
