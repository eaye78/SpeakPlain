use std::time::Duration;
use tauri::{Emitter, Manager, State};
use log::info;
use crate::app_state::AppState;
use super::PipelineContext;

/// Stage 1: 语音活动检测 + ASR 识别
pub async fn run_asr(ctx: &mut PipelineContext) -> anyhow::Result<()> {
    let state: State<AppState> = ctx.app_handle.state();
    let vad_threshold = state.config.lock().vad_threshold;
    let rms = crate::audio::AudioRecorder::calculate_rms(&ctx.audio_samples);
    info!("VAD 检测: 样本数={}, RMS={:.6}, 阈值={:.6}", ctx.audio_samples.len(), rms, vad_threshold);
    if ctx.audio_samples.is_empty() {
        info!("无语音活动（样本数为0），跳过识别");
        state.indicator.lock().set_no_voice();
        state.indicator.lock().hide_delayed(1500);
        return Err(anyhow::anyhow!("无语音活动"));
    }
    if rms < vad_threshold {
        info!("无语音活动（RMS {:.6} < 阈值 {:.6}），跳过识别", rms, vad_threshold);
        state.indicator.lock().set_no_voice();
        state.indicator.lock().hide_delayed(1500);
        return Err(anyhow::anyhow!("无语音活动"));
    }
    info!("检测到语音活动，开始识别...");
    state.indicator.lock().set_processing();
    let text = {
        let engine_arc = state.asr_engine.clone();
        let audio_clone = ctx.audio_samples.clone();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            if engine_arc.lock().is_some() { break; }
            if tokio::time::Instant::now() >= deadline {
                log::error!("等待 ASR 引擎超时（300s），放弃识别");
                state.indicator.lock().set_error("引擎超时");
                state.indicator.lock().hide_delayed(2000);
                return Err(anyhow::anyhow!("ASR引擎超时"));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        match tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let engine = engine_arc.lock();
            match engine.as_ref() {
                Some(e) => {
                    info!("调用 recognize，样本数: {}", audio_clone.len());
                    let result = e.recognize(&audio_clone);
                    info!("recognize 返回: {:?}", result);
                    result
                }
                None => {
                    log::error!("ASR 引擎未初始化");
                    Err(anyhow::anyhow!("引擎未初始化"))
                }
            }
        }).await {
            Ok(Ok(t)) => t,
            Ok(Err(err)) => {
                log::error!("识别失败: {}", err);
                state.indicator.lock().set_error("错误");
                state.indicator.lock().hide_delayed(2000);
                return Err(err);
            }
            Err(e) => {
                log::error!("推理线程 panic: {}", e);
                state.indicator.lock().set_error("错误");
                state.indicator.lock().hide_delayed(2000);
                return Err(anyhow::anyhow!("推理线程 panic: {}", e));
            }
        }
    };
    if text.is_empty() {
        info!("识别结果为空");
        state.indicator.lock().set_no_voice();
        state.indicator.lock().hide_delayed(1500);
        return Err(anyhow::anyhow!("识别结果为空"));
    }
    let cfg = state.config.lock().clone();
    let processed = crate::input::TextProcessor::post_process(&text, cfg.remove_fillers, cfg.capitalize_sentences);
    info!("识别结果: {}", processed);
    ctx.raw_text = text;
    ctx.processed_text = processed;
    ctx.duration_sec = ctx.audio_samples.len() as u32 / 16000;
    Ok(())
}

/// Stage 2: 指令模式匹配
/// 返回 true 表示命中指令，流程应终止
pub async fn run_command(ctx: &mut PipelineContext) -> bool {
    let state: State<AppState> = ctx.app_handle.state();
    let (command_mode_enabled, command_mappings) = {
        let cfg = state.config.lock();
        (cfg.command_mode_enabled, cfg.command_mappings.clone())
    };
    if !command_mode_enabled { return false; }
    if let Some(mapping) = crate::command::find_command_mapping(&ctx.processed_text, &command_mappings) {
        info!("[指令模式] 匹配到指令: {} -> {:?} + {}", mapping.command_text, mapping.modifier, mapping.key_name);
        if let Err(e) = crate::input::execute_command_mapping(mapping) {
            log::error!("[指令模式] 执行按键操作失败: {}", e);
        }
        state.indicator.lock().set_done();
        let auto_hide = state.config.lock().auto_hide_indicator;
        if auto_hide { state.indicator.lock().hide_delayed(1000); }
        ctx.app_handle.emit("recognition:complete", "").ok();
        ctx.command_hit = true;
        return true;
    }
    false
}

/// Stage 3: LLM 润色
pub async fn run_refine(ctx: &mut PipelineContext) {
    let state: State<AppState> = ctx.app_handle.state();
    let cfg = state.config.lock().clone();
    if !cfg.llm_enabled || cfg.llm_provider_id.is_empty() {
        ctx.final_text = ctx.processed_text.clone();
        return;
    }
    let provider_cfg_opt = {
        let storage = state.storage.lock();
        let all_providers = storage.get_llm_providers().ok().unwrap_or_default();
        all_providers.into_iter().find(|p| p.id == cfg.llm_provider_id)
    };
    let provider_cfg = match provider_cfg_opt {
        Some(p) => p,
        None => {
            ctx.final_text = ctx.processed_text.clone();
            return;
        }
    };
    let persona = {
        let custom_personas = state.storage.lock().get_custom_personas().unwrap_or_default();
        let custom_ids: std::collections::HashSet<String> = custom_personas.iter().map(|p| p.id.clone()).collect();
        let all_personas: Vec<crate::llm::Persona> = crate::llm::builtin_personas().into_iter()
            .filter(|p| !custom_ids.contains(&p.id))
            .chain(custom_personas.into_iter())
            .collect();
        all_personas.into_iter()
            .find(|p| p.id == cfg.persona_id)
            .unwrap_or_else(|| crate::llm::builtin_personas().into_iter().find(|p| p.id == "formal").unwrap())
    };
    state.indicator.lock().set_refining();
    match crate::llm::do_refine(&provider_cfg, &persona, &ctx.processed_text).await {
        Ok(refined) => {
            info!("[LLM] 润色成功，结果: {:?}", refined);
            ctx.final_text = refined.clone();
            ctx.llm_refined_text = Some(refined);
            ctx.llm_success = true;
        }
        Err(e) => {
            log::warn!("[LLM] 润色失败，降级使用原始文字: {}", e);
            state.indicator.lock().set_refine_failed("润色失败，已粘贴原文");
            tokio::time::sleep(Duration::from_millis(800)).await;
            ctx.final_text = ctx.processed_text.clone();
            ctx.llm_success = false;
        }
    }
}

/// Stage 4: 保存历史 + 粘贴输出
pub async fn run_output(ctx: &mut PipelineContext) -> anyhow::Result<()> {
    let state: State<AppState> = ctx.app_handle.state();
    let cfg = state.config.lock().clone();
    {
        let storage = state.storage.lock();
        let persona_id = if cfg.llm_enabled { Some(cfg.persona_id.as_str()) } else { None };
        let provider_name: Option<String> = if cfg.llm_enabled && !cfg.llm_provider_id.is_empty() {
            storage.get_llm_providers().ok()
                .and_then(|ps: Vec<crate::llm::LlmProviderConfig>| ps.into_iter().find(|p| p.id == cfg.llm_provider_id))
                .map(|p| p.name)
        } else {
            None
        };
        let _ = storage.add_history_with_llm(
            &ctx.final_text,
            ctx.duration_sec,
            Some(&ctx.processed_text),
            ctx.llm_refined_text.as_deref(),
            persona_id,
            provider_name.as_deref(),
            ctx.llm_success,
        );
    }
    {
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        loop {
            if !state.hotkey_manager.lock().is_key_pressed() { break; }
            if std::time::Instant::now() >= deadline { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    #[cfg(windows)]
    {
        let hwnd = *state.target_hwnd.lock();
        if hwnd != 0 {
            extern "system" { fn SetForegroundWindow(hwnd: isize) -> i32; }
            unsafe { SetForegroundWindow(hwnd); }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    {
        let mut paster = state.input_paster.lock();
        if let Err(e) = paster.paste(&ctx.final_text) {
            log::warn!("粘贴板粘贴失败，尝试直接输入: {}", e);
            let _ = paster.type_text(&ctx.final_text);
        }
    }
    state.indicator.lock().set_done();
    let auto_hide = state.config.lock().auto_hide_indicator;
    if auto_hide { state.indicator.lock().hide_delayed(2000); }
    ctx.app_handle.emit("recognition:complete", &ctx.final_text).ok();
    Ok(())
}
