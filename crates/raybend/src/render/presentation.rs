//! 窗口呈现的最小平台接缝；不拥有设备、窗口或渲染循环。
//!
//! Windows 编辑器使用独立的 DirectComposition visual，隔离 Tauri 的 GDI 清底。
//! macOS/Linux 暂用 wgpu 默认策略；真正适配时从这里增加平台策略，共用 GpuContext。

/// 呈现适配器：仅选择 wgpu instance 的后端与呈现配置。
/// 与 GPU 硬件 adapter、表面的透明度策略是三件不同的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationAdapter {
    /// Windows 产品路径：DX12 + DirectComposition，位于 WebView 子窗下方。
    WindowsComposition,
    /// wgpu 原生默认；用于 spike 和尚未产品化的其它平台。
    PlatformDefault,
}

impl PresentationAdapter {
    /// 产品入口。其它平台目前仅保留接入位置，不代表已经通过桌面合成验证。
    pub fn for_editor() -> Self {
        if cfg!(target_os = "windows") {
            Self::WindowsComposition
        } else {
            Self::PlatformDefault
        }
    }

    pub(crate) fn instance_descriptor(self) -> wgpu::InstanceDescriptor {
        // 默认先设、诊断环境最后覆盖。不要吞掉显式的 WGPU_BACKEND /
        // WGPU_DX12_PRESENTATION_SYSTEM，也不自动退回已知会争用 HWND 的路径。
        self.defaults().with_env()
    }

    fn defaults(self) -> wgpu::InstanceDescriptor {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        if self == Self::WindowsComposition {
            descriptor.backends = wgpu::Backends::DX12;
            descriptor.backend_options.dx12.presentation_system =
                wgpu::Dx12SwapchainKind::DxgiFromVisual;
        }
        descriptor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_uses_dx12_and_an_isolated_visual() {
        // 仅切 DX12 不够：默认 HWND swapchain 仍与 GDI 共用底层。
        // 策略与宿主平台无关，因此 Linux 单测也能守住 Windows 的配置。
        let descriptor = PresentationAdapter::WindowsComposition.defaults();
        assert_eq!(descriptor.backends, wgpu::Backends::DX12);
        assert_eq!(
            descriptor.backend_options.dx12.presentation_system,
            wgpu::Dx12SwapchainKind::DxgiFromVisual,
        );
    }

    #[test]
    fn platform_default_does_not_force_windows_configuration() {
        let expected = wgpu::InstanceDescriptor::new_without_display_handle();
        let actual = PresentationAdapter::PlatformDefault.defaults();
        assert_eq!(actual.backends, expected.backends);
        assert_eq!(
            actual.backend_options.dx12.presentation_system,
            expected.backend_options.dx12.presentation_system,
        );
    }

    #[test]
    fn editor_selects_the_host_platform_policy() {
        let expected = if cfg!(target_os = "windows") {
            PresentationAdapter::WindowsComposition
        } else {
            PresentationAdapter::PlatformDefault
        };
        assert_eq!(PresentationAdapter::for_editor(), expected);
    }
}
