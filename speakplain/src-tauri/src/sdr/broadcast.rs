use crate::sdr::{DemodMode, SdrManager};

/// 【临时测试】一键收听 FM 广播
/// 参数：WFM / 250kHz / 手动增益19.7dB / 禁用CTCSS
pub fn run_broadcast_test(manager: &SdrManager, freq_mhz: f64) -> anyhow::Result<()> {
    // 停止当前流（如果正在运行）
    manager.stop_stream().ok();

    // 固定设置广播测试参数
    manager.set_frequency(freq_mhz)?;
    manager.set_demod_mode(DemodMode::Wbfm);
    manager.set_bandwidth(250_000)?;
    manager.set_ctcss_tone(0.0);
    manager.set_auto_gain(false)?;
    manager.set_gain(19.7)?;
    manager.set_ppm_correction(0)?;

    // 如果设备未连接，自动连接第一个可用设备
    if !manager.is_device_connected() {
        let devices = manager.list_devices()?;
        if devices.is_empty() {
            anyhow::bail!("未检测到RTL-SDR设备，请先插入设备");
        }
        manager.connect(devices[0].index)?;
    }

    // 启动流
    manager.start_stream()?;

    log::info!("[广播测试] 已启动 FM {:.3}MHz WFM 250kHz 增益19.7dB", freq_mhz);
    Ok(())
}
