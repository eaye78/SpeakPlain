//! SDR设备管理模块
//! 支持RTL2832U设备，通过 rtl_sdr 进程读取IQ数据，参考ShinySDR架构实现DSP管线
//!
//! DSP管线：IQ原始数据 → NBFM解调 → FIR低通滤波 → 降采样(2.4MHz→16kHz) → VAD检测 → 输出
//!
//! 使用 rtl_sdr.exe（项目sdr/目录内置）直接读取USB设备数据，不使用网络TCP模式。

pub mod types;
pub(crate) mod hw;
pub mod dsp;
pub mod ctcss;
pub mod manager;
pub mod broadcast;

pub use types::{
    SdrDeviceInfo, SdrStatus, DemodMode, InputSource, TestResult,
};
pub use manager::SdrManager;
