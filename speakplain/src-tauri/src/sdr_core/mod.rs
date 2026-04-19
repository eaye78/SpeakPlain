//! SDR 核心模块 - 使用 librtlsdr 直接调用
//! 
//! 参考 SDR++ 架构：
//! 1. 使用 rtlsdr_read_async() 异步读取 IQ 数据
//! 2. 实现流式 DSP 处理管线
//! 3. IQ 域 CTCSS 检测（DDC + FM 解调 + 频率估计）

pub mod device;
pub mod stream;
pub mod dsp;
pub mod ctcss;

pub use device::RtlSdrDevice;
pub use stream::IQStream;
pub use dsp::{Ddc, FmDemod, DspPipeline};
pub use ctcss::CtcssDetector;
