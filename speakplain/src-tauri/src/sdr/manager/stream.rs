//! SDR 音频流管理

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapRb, traits::{Consumer, Observer, Producer, Split}};
use std::sync::atomic::Ordering;

use super::SdrManager;
use crate::sdr::types::StreamHandle;
use crate::sdr::hw::start_read_thread;
use crate::sdr::dsp::DspPipeline;
use crate::sdr::ctcss::CtcssDetector;

impl SdrManager {
    /// 启动IQ数据采集与DSP处理流（核心管线）
    ///
    /// 工作原理：
    /// 1. 开启cpal音频输出流（用于将解调音频路由到VB-Cable等虚拟麦克风）
    /// 2. 真实硬件模式下，另起线程读取RTL-SDR的IQ数据，经DSP管线处理后
    ///    写入音频输出流并存入audio_buffer供ASR消费
    pub fn start_stream(&self) -> Result<()> {
        if self.streaming.load(Ordering::Relaxed) {
            return Ok(());
        }
        if !self.is_device_connected() {
            anyhow::bail!("SDR设备未连接");
        }

        let host = cpal::default_host();
        let output_device = {
            let target_name = self.config.lock().output_device.clone();
            if target_name.is_empty() {
                host.default_output_device().context("无法获取默认输出设备")?
            } else {
                let mut devices = host.output_devices()?;
                devices
                    .find(|d| d.name().map(|n| n == target_name).unwrap_or(false))
                    .context(format!("输出设备 '{}' 未找到", target_name))?
            }
        };

        // 对齐 SDR++ audio_sink：
        //   - 强制立体声2ch，采样率48000Hz（对应 Realtek Audio 硬件配置）
        //   - bufferFrames = sampleRate / 60 ≈ 800帧 (~16.7ms)，最小化延迟
        //   - 使用 RTAUDIO_MINIMIZE_LATENCY 等价：cpal Default buffer size
        //   - ringbuf 存立体声 interleaved f32，回调直接 copy_from_slice（等价 memcpy）
        const OUT_CHANNELS: usize = 2;
        const OUT_SAMPLE_RATE: u32 = 48000;
        // SDR++ bufferFrames = sampleRate / 60
        let buffer_frames = OUT_SAMPLE_RATE / 60; // 800帧 = 16.7ms
        let stream_config = cpal::StreamConfig {
            channels: OUT_CHANNELS as u16,
            sample_rate: cpal::SampleRate(OUT_SAMPLE_RATE),
            // Default 让驱动自选最小延迟，对应 SDR++ RTAUDIO_MINIMIZE_LATENCY
            buffer_size: cpal::BufferSize::Fixed(buffer_frames),
        };
        let out_sample_rate = OUT_SAMPLE_RATE;
        let out_channels = OUT_CHANNELS;
        log::info!("[SDR音频] SDR++对齐 - 设备:{} 采样率:{}Hz {}ch bufferFrames={} ({:.1}ms)",
            output_device.name().unwrap_or_default(),
            out_sample_rate, out_channels, buffer_frames,
            buffer_frames as f32 / out_sample_rate as f32 * 1000.0);
        self.out_sample_rate.store(out_sample_rate, Ordering::Relaxed);

        // SDR++ stereoPacker 等价：ringbuf 存立体声 interleaved (L, R) = (sample, sample)
        // 容量 = 1秒立体声样本（每帧2个f32），约够60次回调缓冲
        let queue_cap = out_sample_rate as usize * out_channels; // 1秒立体声
        let ring = HeapRb::<f32>::new(queue_cap);
        let (mut audio_prod, mut audio_cons) = ring.split();
        let audio_queue_len_ref = self.audio_queue_len.clone();

        // DSP配置快照
        let cfg_snap = self.config.lock().clone();

        // SDR++ callback 等价：
        //   count = stereoPacker.out.read();
        //   memcpy(outputBuffer, readBuf, nBufferFrames * sizeof(stereo_t));
        // Rust实现：从无锁ringbuf批量读取立体声样本直接填充data切片
        let stream = output_device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // data.len() = buffer_frames * channels (interleaved stereo)
                let available = audio_cons.occupied_len();
                if available < data.len() {
                    // 欠载：用静音填充缺失部分
                    if available == 0 {
                        // 完全欠载：静音（对应 SDR++ 无数据时不输出噪声）
                        data.fill(0.0);
                        return;
                    }
                    log::warn!("[音频欠载] 环形缓冲仅{}个f32，需{}个，补静音", available, data.len());
                }
                // SDR++ 等价 memcpy：直接从ringbuf批量复制到输出缓冲
                let copied = audio_cons.pop_slice(data);
                // 若不够则用0填充剩余（上面欠载判断已覆盖，这里做保底）
                if copied < data.len() {
                    data[copied..].fill(0.0);
                }
            },
            move |err| {
                log::error!("SDR音频输出流错误: {}", err);
            },
            None,
        )?;
        // 先标记 streaming 为 true，再启动读取线程，让数据先开始积累
        self.streaming.store(true, Ordering::Relaxed);

        // 启动IQ读取线程（直接调用 librtlsdr read_sync）
        let device_handle = {
            let dev = self.device.lock();
            dev.as_ref().cloned().context("SDR设备未连接")?
        };
        let streaming_flag = self.streaming.clone();
        let sample_rate = cfg_snap.sample_rate;
        let demod_mode = cfg_snap.demod_mode.clone();
        let audio_out_rate = out_sample_rate;
        let bandwidth = cfg_snap.bandwidth;
        let call_test_mode_flag = self.call_test_mode.clone();
        let audio_queue_len_flag = audio_queue_len_ref.clone();
        let signal_ref = self.signal_strength_raw.clone();
        let audio_buffer_ref = self.audio_buffer.clone();
        let vad_ref = self.vad_active.clone();
        let diag_audio_rms_ref = self.diag_audio_rms.clone();
        let diag_iq_range_ref = self.diag_iq_range.clone();
        let diag_iq_dc_i_ref = self.diag_iq_dc_i.clone();
        let ctcss_detected_ref = self.ctcss_detected.clone();
        let ctcss_strength_ref = self.ctcss_strength.clone();
        // 传入实时配置Arc，让DSP回调可以读取最新的ctcss_tone/ctcss_threshold
        let config_arc = self.config.clone();
        let _vad_threshold = cfg_snap.vad_threshold;
        let on_speech_end_ref = self.on_speech_end.clone();
        let on_signal_ref = self.on_signal.clone();
        let on_vad_change_ref = self.on_vad_change.clone();

        let mut pipeline = DspPipeline::new(sample_rate, audio_out_rate, demod_mode, bandwidth);
        let mut prev_vad = false;
        let silence_timeout_frames: u32 = 3;
        let mut silence_frames: u32 = 0;
        let mut first_frame_logged = false;
        let mut ctcss_detector: Option<CtcssDetector> = None;
        let mut last_iq_time = std::time::Instant::now();

        let cmd_tx = start_read_thread(
            &device_handle,
            &cfg_snap,
            streaming_flag.clone(),
            move |iq_bytes: &[u8]| {
                let _iq_interval = last_iq_time.elapsed();
                last_iq_time = std::time::Instant::now();
                let dsp_output = pipeline.process(iq_bytes);
                let _process_time = last_iq_time.elapsed();
                let rms = pipeline.signal_rms;
                signal_ref.store(rms.to_bits(), Ordering::Relaxed);
                
                // 提取音频样本和IQ样本用于后续处理
                // Squelch 静噪：基于 IQ RMS 判断有无信号
                // 修复：阈值从 0.03 提高到 0.15。噪声 floor RMS≈sqrt(0.0079)=0.089，
                // 原阈值 0.03 低于噪声 floor，导致 squelch 始终开启、持续输出噪声。
                // 0.15 略高于噪声 floor，确保无信号时 squelch 关闭。
                let squelch_open = pipeline.signal_rms > 0.15;
                let audio_samples: Vec<f32> = dsp_output.audio;
                let _iq_samples = dsp_output.iq_samples;

                // CTCSS 检测处理（Goertzel：使用 FM 解调后的频偏样本）
                // 实时读取配置，确保流运行期间修改亚音设置立即生效
                let (ctcss_tone, ctcss_threshold) = {
                    let cfg = config_arc.lock();
                    (cfg.ctcss_tone, cfg.ctcss_threshold)
                };
                // 始终初始化并运行CTCSS检测器（方便诊断），但只在Squelch开启时信任结果
                let (ctcss_detected, ctcss_strength) = if ctcss_tone > 0.0 {
                    // 延迟初始化检测器（使用stage1_rate采样率）
                    if ctcss_detector.is_none() {
                        ctcss_detector = Some(CtcssDetector::new(ctcss_tone, pipeline.stage1_rate as f32, ctcss_threshold));
                        log::info!("CTCSS检测器(Goertzel)初始化: {} Hz, 输入采样率 {} Hz, 门限={}", 
                            ctcss_tone, pipeline.stage1_rate, ctcss_threshold);
                    }
                    if let Some(ref mut detector) = ctcss_detector {
                        // 处理频偏样本（Goertzel 直接检测目标频率）
                        detector.process(&dsp_output.freq_samples);
                        if squelch_open {
                            // 有信号：使用检测器结果
                            (detector.detected, detector.strength)
                        } else {
                            // 无信号：显示检测器结果但标记为未检测（避免false positive）
                            (false, detector.strength)
                        }
                    } else {
                        (false, 0.0)
                    }
                } else {
                    // 未设置CTCSS，始终返回检测通过
                    (true, 1.0)
                };

                // 存储 CTCSS 检测状态
                ctcss_detected_ref.store(ctcss_detected, Ordering::Relaxed);
                ctcss_strength_ref.store(ctcss_strength.to_bits(), Ordering::Relaxed);

                // CTCSS 静音控制：设置了亚音且未检测到时静音
                let ctcss_mute = ctcss_tone > 0.0 && !ctcss_detected;
                
                // SDR++ 风格：如果设置了亚音但未检测到，静音音频（输出0）
                let audio_samples_filtered: Vec<f32> = if ctcss_mute {
                    // 静音：输出全0（参考 SDR++ memset(out, 0, ...)）
                    vec![0.0f32; audio_samples.len()]
                } else {
                    // 直通：原样输出（参考 SDR++ memcpy(out, in, ...)）
                    audio_samples.clone()
                };

                // 首帧异常诊断：仅在有明显问题时输出 warn
                if !first_frame_logged {
                    first_frame_logged = true;
                    let mean_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0] as f32).sum::<f32>() / 500.0;
                    let max_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0]).max().unwrap_or(0);
                    let min_i = iq_bytes.chunks_exact(2).take(500)
                        .map(|c| c[0]).min().unwrap_or(255);
                    if (mean_i - 127.4).abs() > 10.0 {
                        log::warn!("[首帧诊断] IQ均值偏差较大（{:.1}），可能为全0或全127，请检查rtl_sdr连接。", mean_i);
                    }
                    if (max_i as i16 - min_i as i16) < 5 {
                        log::warn!("[首帧诊断] IQ范围过小（{}），可能设备没有在返回IQ数据！", max_i as i16 - min_i as i16);
                    }
                    if audio_samples.is_empty() {
                        log::warn!("[首帧诊断] 音频样本为空，请检查FM解调是否正常");
                    }
                    if dsp_output.freq_samples.is_empty() {
                        log::warn!("[首帧诊断] 频偏样本为空，CTCSS检测器无输入！");
                    }
                }

                // 更新诊断指标
                diag_audio_rms_ref.store(pipeline.diag_audio_rms.to_bits(), Ordering::Relaxed);
                diag_iq_range_ref.store(pipeline.diag_iq_range.to_bits(), Ordering::Relaxed);
                diag_iq_dc_i_ref.store(pipeline.diag_iq_dc_i.to_bits(), Ordering::Relaxed);

                // 帧处理完成

                // 推送信号强度回调：基于实际输出到扬声器的音频（audio_samples_filtered）
                // 与扬声器声音保持一致：CTCSS静音时为0，有声音时随音量变化
                let audio_out_rms = if audio_samples_filtered.is_empty() {
                    0.0f32
                } else {
                    (audio_samples_filtered.iter().map(|&x| x * x).sum::<f32>() / audio_samples_filtered.len() as f32).sqrt()
                };
                let signal_val = (audio_out_rms * 6.0).powf(0.65).min(1.0);
                if let Some(ref cb) = *on_signal_ref.lock() {
                    cb(signal_val);
                }

                // VAD 检测：直接使用 squelch 状态（基于 IQ 功率）
                // IQ 功率无人发射时 ≈0.017，有人发射时 ≈1.4，区分度远优于音频 RMS
                // squelch_open 阈值 0.15 已在上方基于实测噪底校准，无需用户调整
                let has_voice = squelch_open;

                // ── PTT 静音延迟逻辑 ──────────────────────────────────────
                // 当前帧有语音：重置静音计数，标记 VAD 活跃
                // 当前帧无语音：累加静音帧数，达到阈值才真正结束语音段
                // 这样可以容忍字间短暂停顿，只有 PTT 松开（信号彻底消失）
                // 连续 silence_timeout_frames 帧才触发 on_speech_end
                let was_in_silence_window = prev_vad && silence_frames > 0;
                let vad_active_now = if has_voice {
                    silence_frames = 0;
                    true
                } else if prev_vad {
                    silence_frames += 1;
                    silence_frames < silence_timeout_frames  // 还在延迟窗口内，保持活跃
                } else {
                    silence_frames = 0;
                    false
                };

                vad_ref.store(vad_active_now, Ordering::Relaxed);

                // VAD状态变化时推送回调
                // 特殊情况：在静音延迟窗口内重新检测到语音（PTT松开后立即再按），
                // vad_active_now 保持 true 不变，但需要通知前端重新开始计时
                if vad_active_now != prev_vad {
                    if let Some(ref cb) = *on_vad_change_ref.lock() {
                        cb(vad_active_now);
                    }
                } else if vad_active_now && was_in_silence_window && has_voice {
                    // 静音窗口内重新有信号：补发一次 true，让前端重置计时器
                    if let Some(ref cb) = *on_vad_change_ref.lock() {
                        cb(true);
                    }
                }

                // 推送音频样本到无锁环形缓冲区（DSP线程写，cpal回调读）
                // SDR++ stereoPacker 等价：单声道样本展开为立体声 interleaved (L=s, R=s)
                {
                    let n = audio_samples_filtered.len();
                    // 预分配立体声缓冲：每个单声道样本展开为 2 个 f32
                    let stereo_len = n * 2;
                    let free = audio_prod.vacant_len();
                    if free < stereo_len {
                        // 环形缓冲已满：跳过本批数据（保持实时性，避免延迟累积）
                        log::debug!("[音频丢弃] 环形缓冲已满 ({} free, need {})，丢弃本批", free, stereo_len);
                    } else {
                        // SDR++ stereoPacker等价：单声道 -> 立体声 interleaved
                        // 直接展开写入ringbuf，避免中间Vec分配
                        for &s in &audio_samples_filtered {
                            // (L, R) = (s, s) 对应 SDR++ stereo_t{l: s, r: s}
                            let _ = audio_prod.try_push(s);
                            let _ = audio_prod.try_push(s);
                        }
                        // 更新队列占用长度（单位：f32个数，包含左右声道）
                        audio_queue_len_flag.store(
                            (audio_prod.occupied_len() / 2) as u32, // 头投为帧数
                            Ordering::Relaxed);
                    }
                }

                // 语音活跃（含延迟窗口内）时累积音频用于 ASR
                // 参考 SDR++：CTCSS 只控制静音，不影响语音检测和累积
                // 累积原始音频（不应用 CTCSS 静音，确保 ASR 能接收到完整语音）
                if vad_active_now {
                    let mut buf = audio_buffer_ref.lock();
                    buf.extend_from_slice(&audio_samples);
                }

                // 语音段真正结束：prev_vad 活跃 → 当前已不活跃（超出延迟窗口）
                // 参考 SDR++：CTCSS 只控制静音开关，不影响 PTT 触发
                if prev_vad && !vad_active_now {
                    let audio_data = {
                        let mut buf = audio_buffer_ref.lock();
                        std::mem::take(&mut *buf)
                    };
                    
                    // 通话测试模式下不触发 ASR
                    if !call_test_mode_flag.load(Ordering::Relaxed) {
                        // 检查音频长度是否足够（至少300ms）
                        let min_samples = (audio_out_rate as f32 * 0.3) as usize;
                        
                        if !audio_data.is_empty() && audio_data.len() >= min_samples {
                            log::info!("PTT语音段结束，触发ASR，样本数={}", audio_data.len());
                            if let Some(ref cb) = *on_speech_end_ref.lock() {
                                cb(audio_data);
                            }
                        } else if !audio_data.is_empty() {
                            log::debug!("音频段过短，跳过ASR，样本数={}", audio_data.len());
                        }
                    } else {
                        // 通话测试模式：清空缓冲，不触发 ASR
                        drop(audio_data); // 直接丢弃
                        log::info!("PTT语音段结束（通话测试模式，跳过ASR）");
                    }
                }
                prev_vad = vad_active_now;
            },
        ).context("启动RTL-SDR读取线程失败")?;
        *self.cmd_tx.lock() = Some(cmd_tx);

        // 直接启动音频流（无需预填充，无锁设计天然支持起始欠载）
        stream.play()?;
        *self.audio_stream.lock() = Some(StreamHandle(stream));

        log::info!("SDR音频流已启动 (音频设备: {}Hz {}ch ringbuf容量={})",
            out_sample_rate, out_channels, queue_cap);
        Ok(())
    }

    /// 停止音频流
    pub fn stop_stream(&self) -> Result<()> {
        if !self.streaming.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.streaming.store(false, Ordering::Relaxed);
        // 等待IQ读取线程退出（最多500ms）
        std::thread::sleep(std::time::Duration::from_millis(50));
        *self.audio_stream.lock() = None;
        *self.cmd_tx.lock() = None; // Drop sender → read thread channel closed
        self.vad_active.store(false, Ordering::Relaxed);
        log::info!("SDR音频流已停止");
        Ok(())
    }

    /// 取出ASR音频缓冲（消费式，取出后清空）
    pub fn take_audio_buffer(&self) -> Vec<f32> {
        let mut buf = self.audio_buffer.lock();
        std::mem::take(&mut *buf)
    }
}
