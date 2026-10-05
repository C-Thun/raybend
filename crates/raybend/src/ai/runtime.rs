//! 精确锁定的 CPU 图像编码器；前端与标签真相层均不持有 ORT 类型。
use crate::{Error, Result};
use ort::{ep, session::Session, value::Tensor};
use std::path::Path;
fn failure(e: impl std::fmt::Display) -> Error {
    Error::Unsupported(format!("AI CPU 推理：{e}"))
}
pub struct Encoder {
    session: Session,
    dimensions: usize,
}
impl Encoder {
    pub fn load(library: &Path, model: &Path) -> Result<Self> {
        Self::load_sized(library, model, 512)
    }
    /// 768 仅供 SigLIP 对照，不改变 512 的生产模型契约。
    pub fn load_sized(library: &Path, model: &Path, dimensions: usize) -> Result<Self> {
        if ![512, 768].contains(&dimensions) {
            return Err(failure("不支持的图像特征维数"));
        }
        if !library.is_absolute() || !library.is_file() || !model.is_absolute() || !model.is_file()
        {
            return Err(failure("运行库与模型必须是存在的绝对路径"));
        }
        ort::init_from(library)
            .map_err(failure)?
            .with_name("raybend-ai")
            .with_execution_providers([ep::CPU::default().build()])
            .commit();
        let session = Session::builder()
            .map_err(failure)?
            .with_execution_providers([ep::CPU::default().build()])
            .map_err(failure)?
            .with_intra_threads(2)
            .map_err(failure)?
            .with_inter_threads(1)
            .map_err(failure)?
            .with_parallel_execution(false)
            .map_err(failure)?
            .commit_from_file(model)
            .map_err(failure)?;
        let mut encoder = Self {
            session,
            dimensions,
        };
        // 加载时实际探测图契约。manifest 中的名字/shape 声明不能代替模型验证。
        encoder.encode(vec![0.; 3 * 224 * 224])?;
        Ok(encoder)
    }
    pub fn encode(&mut self, pixels: Vec<f32>) -> Result<Vec<f32>> {
        if pixels.len() != 3 * 224 * 224 || pixels.iter().any(|v| !v.is_finite()) {
            return Err(failure("输入张量无效"));
        }
        let tensor =
            Tensor::from_array(([1, 3, 224, 224], pixels.into_boxed_slice())).map_err(failure)?;
        let values = self
            .session
            .run(ort::inputs!["pixel_values"=>&tensor])
            .map_err(failure)?;
        let feature = values
            .get("image_features")
            .ok_or_else(|| failure("缺图像特征输出"))?;
        let (shape, data) = feature.try_extract_tensor::<f32>().map_err(failure)?;
        if shape.as_ref() != [1, self.dimensions as i64] || data.iter().any(|v| !v.is_finite()) {
            return Err(failure("图像特征形状/数值无效"));
        }
        Ok(data.to_vec())
    }
}
