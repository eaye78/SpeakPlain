// 音频录制模块
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use log::info;
use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// 目标采样率（SenseVoice 要求 16kHz）
const TARGET_SAMPLE_RATE: u32 = 16000;

/// 音量回调类型，参数为 RMS 音量 (0.0 ~ 1.0)
pub type VolumeCallback = Arc<dyn Fn(f32) + Send + Sync>;

/// 将 cpal::Stream 包装为 Send，安全性由 parking_lot::Mutex 保证
struct SendStream(cpal::Stream);
unsafe impl Send for SendStream {}

pub struct AudioRecorder {
    device: cpal::Device,
    /// 设备原生配置（真实支持的采样率/声道数）
    native_config: cpal::StreamConfig,
    /// 设备原生采样率
    native_sample_rate: u32,
    /// 设备原生声道数
    native_channels: u16,
    stream: Option<SendStream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    /// 可选音量回调，每个音频块触发一次
    volume_callback: Option<VolumeCallback>,
}

impl AudioRecorder {
    pub fn new() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("未找到麦克风设备，请检查系统音频输入设置"))?;

        let supported = device.default_input_config()
            .map_err(|e| anyhow::anyhow!("获取麦克风配置失败: {}\n请确认麦克风已连接并授权使用", e))?;

        let native_sample_rate = supported.sample_rate().0;
        let native_channels   = supported.channels();

        info!("麦克风: {:?}, 采样率: {}Hz, 声道: {}, 格式: {:?}",
            device.name(), native_sample_rate, native_channels, supported.sample_format());

        let native_config = cpal::StreamConfig {
            channels:    native_channels,
            sample_rate: cpal::SampleRate(native_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(Self {
            device,
            native_config,
            native_sample_rate,
            native_channels,
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(AtomicBool::new(false)),
            volume_callback: None,
        })
    }

    /// 注册音量回调，录音期间每个音频块都会触发
    pub fn set_volume_callback(&mut self, cb: VolumeCallback) {
        self.volume_callback = Some(cb);
    }

    /// 计算 RMS 音量
    pub fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() { return 0.0; }
        let sum: f32 = samples.iter().map(|&s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    /// 检测样本中是否存在语音活动（RMS > threshold）
    pub fn has_voice_activity(samples: &[f32], threshold: f32) -> bool {
        Self::calculate_rms(samples) > threshold
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.is_recording.load(Ordering::Relaxed) {
            return Ok(());
        }

        info!("开始音频录制（原生 {}Hz {}ch → 重采样到 16kHz 单声道）",
            self.native_sample_rate, self.native_channels);

        self.buffer.lock().clear();

        let buffer          = self.buffer.clone();
        let volume_callback = self.volume_callback.clone();
        let native_sr       = self.native_sample_rate;
        let channels        = self.native_channels as usize;

        // 获取设备支持的采样格式，用于决定如何转换
        let sample_format = self.device.default_input_config()
            .map(|c| c.sample_format())
            .unwrap_or(SampleFormat::F32);

        // 构建重采样器（仅当原生采样率 ≠ 16kHz 时使用）
        // 使用累积输入缓冲区，解决 cpal 每次回调帧数不足 chunk_size 的问题
        let resampler: Option<Arc<Mutex<SincFixedIn<f32>>>> =
            if native_sr != TARGET_SAMPLE_RATE {
                let params = SincInterpolationParameters {
                    sinc_len: 128,
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 128,
                    window: WindowFunction::BlackmanHarris2,
                };
                let chunk = 512usize; // 更小的 chunk，更容易积累到足够数据
                let r = SincFixedIn::<f32>::new(
                    TARGET_SAMPLE_RATE as f64 / native_sr as f64,
                    2.0,
                    params,
                    chunk,
                    1,
                ).map_err(|e| anyhow::anyhow!("创建重采样器失败: {:?}", e))?;
                Some(Arc::new(Mutex::new(r)))
            } else {
                None
            };

        // 重采样输入累积缓冲区（解决 cpal 小块回调问题）
        let resample_inbox: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));

        // 构建通用 f32 回调
        let make_callback = move |data_f32: Vec<f32>| {
            // 1. 触发音量回调
            if let Some(ref cb) = volume_callback {
                let rms = Self::calculate_rms(&data_f32);
                cb(rms);
            }

            // 2. 多声道 → 单声道（均值混音）
            let mono: Vec<f32> = if channels == 1 {
                data_f32
            } else {
                data_f32.chunks(channels)
                    .map(|ch| ch.iter().sum::<f32>() / channels as f32)
                    .collect()
            };

            // 3. 重采样到 16kHz（如需要）
            let resampled = if let Some(ref resampler_arc) = resampler {
                let mut r = resampler_arc.lock();
                let chunk_size = r.input_frames_next();

                // 累积到 inbox，再批量处理
                let mut inbox = resample_inbox.lock();
                inbox.extend_from_slice(&mono);

                let mut output = Vec::new();
                while inbox.len() >= chunk_size {
                    let slice: Vec<f32> = inbox.drain(..chunk_size).collect();
                    match r.process(&[&slice], None) {
                        Ok(out) => output.extend_from_slice(&out[0]),
                        Err(e) => log::warn!("重采样错误: {:?}", e),
                    }
                }
                output
            } else {
                mono
            };

            // 4. 写入缓冲区，限制最大 60 秒
            if !resampled.is_empty() {
                let mut buf = buffer.lock();
                buf.extend_from_slice(&resampled);
                let max_samples = TARGET_SAMPLE_RATE as usize * 60;
                if buf.len() > max_samples {
                    let drain_to = buf.len() - max_samples;
                    buf.drain(0..drain_to);
                }
            }
        };

        // 根据设备原生格式构建流
        let stream = match sample_format {
            SampleFormat::F32 => {
                let cb = make_callback;
                self.device.build_input_stream(
                    &self.native_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        cb(data.to_vec());
                    },
                    |err| log::error!("音频流错误: {}", err),
                    None,
                )?
            }
            SampleFormat::I16 => {
                let cb = make_callback;
                self.device.build_input_stream(
                    &self.native_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let f: Vec<f32> = data.iter()
                            .map(|&s| s as f32 / i16::MAX as f32)
                            .collect();
                        cb(f);
                    },
                    |err| log::error!("音频流错误: {}", err),
                    None,
                )?
            }
            SampleFormat::U16 => {
                let cb = make_callback;
                self.device.build_input_stream(
                    &self.native_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let f: Vec<f32> = data.iter()
                            .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .collect();
                        cb(f);
                    },
                    |err| log::error!("音频流错误: {}", err),
                    None,
                )?
            }
            other => {
                return Err(anyhow::anyhow!("不支持的音频格式: {:?}", other));
            }
        };

        stream.play()?;
        self.stream = Some(SendStream(stream));
        self.is_recording.store(true, Ordering::Relaxed);

        info!("音频录制已启动");
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        if !self.is_recording.load(Ordering::Relaxed) {
            return Vec::new();
        }

        info!("停止音频录制");

        if let Some(SendStream(stream)) = self.stream.take() {
            let _ = stream.pause();
        }

        self.is_recording.store(false, Ordering::Relaxed);

        let audio_data = self.buffer.lock().clone();
        self.buffer.lock().clear();

        info!("录制完成，16kHz样本数: {}", audio_data.len());
        audio_data
    }

    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::Relaxed)
    }

    pub fn get_sample_rate(&self) -> u32 {
        TARGET_SAMPLE_RATE
    }

    /// 获取可用音频设备列表
    pub fn list_devices() -> anyhow::Result<Vec<String>> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                devices.push(name);
            }
        }
        Ok(devices)
    }

    /// 切换到指定设备
    pub fn set_device(&mut self, device_name: &str) -> anyhow::Result<()> {
        let host = cpal::default_host();
        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                if name == device_name {
                    // 更新设备和原生配置
                    let supported = device.default_input_config()
                        .map_err(|e| anyhow::anyhow!("获取设备配置失败: {}", e))?;
                    self.native_sample_rate = supported.sample_rate().0;
                    self.native_channels   = supported.channels();
                    self.native_config = cpal::StreamConfig {
                        channels:    self.native_channels,
                        sample_rate: cpal::SampleRate(self.native_sample_rate),
                        buffer_size: cpal::BufferSize::Default,
                    };
                    self.device = device;
                    info!("切换到音频设备: {} ({}Hz {}ch)",
                        device_name, self.native_sample_rate, self.native_channels);
                    return Ok(());
                }
            }
        }
        Err(anyhow::anyhow!("未找到设备: {}", device_name))
    }
}

/// 简单的VAD (语音活动检测)
#[allow(dead_code)]
pub struct VadDetector {
    threshold: f32,
    silence_duration: std::time::Duration,
    last_voice_time: Option<std::time::Instant>,
}

#[allow(dead_code)]
impl VadDetector {
    pub fn new(threshold: f32, silence_duration_ms: u64) -> Self {
        Self {
            threshold,
            silence_duration: std::time::Duration::from_millis(silence_duration_ms),
            last_voice_time: None,
        }
    }
    
    /// 计算音频数据的RMS能量
    pub fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        
        let sum: f32 = samples.iter().map(|&s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }
    
    /// 检测是否有语音活动
    pub fn has_voice_activity(&mut self, samples: &[f32]) -> bool {
        let rms = Self::calculate_rms(samples);
        let has_voice = rms > self.threshold;
        
        if has_voice {
            self.last_voice_time = Some(std::time::Instant::now());
        }
        
        has_voice
    }
    
    /// 检查是否静音超时
    pub fn is_silence_timeout(&self) -> bool {
        if let Some(last_voice) = self.last_voice_time {
            last_voice.elapsed() > self.silence_duration
        } else {
            false
        }
    }
    
    pub fn reset(&mut self) {
        self.last_voice_time = None;
    }
}
