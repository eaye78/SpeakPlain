import { useEffect, useRef, useState } from "react";
import { listen, emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "../styles.css";
import { onIndicatorSkinChange, applySkinToIndicator, skinManager } from "../themes";

interface StatusMessage {
  status: string;
  message: string;
}

const BAR_COUNT = 40;

function Indicator() {
  const [status, setStatus] = useState<string>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [procElapsed, setProcElapsed] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const procTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  // Web Audio API refs
  const audioCtxRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const micStreamRef = useRef<MediaStream | null>(null);
  const rafRef = useRef<number | null>(null);
  const waveHistoryRef = useRef<number[]>(Array(BAR_COUNT).fill(0));
  // SDR 模式：由后端信号值直接驱动波形，无需 getUserMedia
  const sdrSignalRef = useRef<number>(0);   // 最新收到的信号强度
  const sdrModeRef = useRef<boolean>(false); // 是否处于 SDR 驱动模式
  // 计时器
  const startTimer = () => {
    setElapsed(0);
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = setInterval(() => setElapsed(s => s + 1), 1000);
  };
  const stopTimer = () => {
    if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; }
  };

  const startProcTimer = () => {
    setProcElapsed(0);
    if (procTimerRef.current) clearInterval(procTimerRef.current);
    procTimerRef.current = setInterval(() => setProcElapsed(s => s + 1), 1000);
  };
  const stopProcTimer = () => {
    if (procTimerRef.current) { clearInterval(procTimerRef.current); procTimerRef.current = null; }
  };

  // 波形绘制（核心 draw 循环，同时支持麦克风和 SDR 两种数据源）
  const drawLoop = () => {
    const COLS = 50;
    const doFrame = () => {
      rafRef.current = requestAnimationFrame(doFrame);
      const canvas = canvasRef.current;
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const cw = canvas.offsetWidth * dpr;
      const ch = canvas.offsetHeight * dpr;
      if (canvas.width !== cw || canvas.height !== ch) {
        canvas.width = cw;
        canvas.height = ch;
      }
      const w = canvas.width, h = canvas.height;
      const gfx = canvas.getContext("2d")!;

      // 采样一帧数据
      const hist = waveHistoryRef.current;
      if (sdrModeRef.current) {
        // SDR 模式：直接使用后端推送的信号值
        hist.push(sdrSignalRef.current);
      } else if (analyserRef.current) {
        // 麦克风模式：Web Audio API 采样
        const timeDomain = new Uint8Array(analyserRef.current.fftSize);
        analyserRef.current.getByteTimeDomainData(timeDomain);
        let sum = 0;
        for (let i = 0; i < timeDomain.length; i++) {
          const v = (timeDomain[i] - 128) / 128;
          sum += v * v;
        }
        const rms = Math.sqrt(sum / timeDomain.length);
        hist.push(Math.min(Math.pow(rms * 6, 0.65), 1.0));
      } else {
        hist.push(0);
      }
      if (hist.length > COLS) hist.shift();

      // 绘制
      gfx.clearRect(0, 0, w, h);
      const gap = 1.5;
      const barW = Math.max((w - gap * (COLS - 1)) / COLS, 1.5);
      const cy = h / 2;
      const waveColor = getComputedStyle(document.documentElement)
        .getPropertyValue("--skin-wave-primary").trim() || "#3b6beb";
      for (let i = 0; i < hist.length; i++) {
        const x = i * (barW + gap);
        const bh = Math.max(hist[i] * h * 0.9, 2);
        const alpha = 0.35 + 0.65 * (i / COLS);
        gfx.globalAlpha = alpha;
        gfx.fillStyle = waveColor;
        gfx.beginPath();
        const r = Math.min(barW / 2, 2);
        gfx.roundRect(x, cy - bh / 2, barW, bh, r);
        gfx.fill();
      }
      gfx.globalAlpha = 1;
    };
    waveHistoryRef.current = Array(COLS).fill(0);
    doFrame();
  };

  const startWaveform = async () => {
    if (rafRef.current) return;
    sdrModeRef.current = false;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
      micStreamRef.current = stream;
      const ctx = new AudioContext();
      audioCtxRef.current = ctx;
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 1024;
      analyser.smoothingTimeConstant = 0.3;
      analyserRef.current = analyser;
      ctx.createMediaStreamSource(stream).connect(analyser);
    } catch (_e) {}
    drawLoop();
  };

  // SDR 模式启动：不申请麦克风，直接用信号值驱动波形
  const startSdrWaveform = () => {
    if (rafRef.current) return;
    sdrModeRef.current = true;
    sdrSignalRef.current = 0;
    drawLoop();
  };
  const stopWaveform = () => {
    if (rafRef.current) { cancelAnimationFrame(rafRef.current); rafRef.current = null; }
    if (audioCtxRef.current) { audioCtxRef.current.close(); audioCtxRef.current = null; }
    if (micStreamRef.current) { micStreamRef.current.getTracks().forEach(t => t.stop()); micStreamRef.current = null; }
    analyserRef.current = null;
  };

  // 拖拽
  const handleDragBarMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    invoke("drag_indicator").catch(() => {});
  };

  // 监听后端状态事件
  useEffect(() => {
    document.body.classList.add("indicator-page");
    
    // 检查 Tauri API 是否可用
    console.log('[Indicator] __TAURI__ available:', !!(window as any).__TAURI__);
    console.log('[Indicator] __TAURI__.core:', !!((window as any).__TAURI__?.core));

    // 悬浮框窗口不直接初始化皮肤系统（因为没有 Tauri API）
    // 而是等待主窗口发送皮肤数据
    console.log('[Indicator] Waiting for skin data from main window...');

    // 直接监听跨窗口皮肤切换事件
    const unlistenSkin = listen<string>("skin:change", async (event) => {
      await skinManager.applySkin(event.payload);
      await applySkinToIndicator();
    });

    // 兜底：skinManager 内部 listener（双保险）
    const unsubscribe = onIndicatorSkinChange(async () => {
      await applySkinToIndicator();
    });

    // 通知后端悬浮框已就绪，并请求当前皮肤数据
    emit("indicator:ready").catch(() => {});
    
    // 延迟一下再请求皮肤数据（确保主窗口已初始化）
    setTimeout(() => {
      emit("indicator:request_skin").catch(() => {});
    }, 100);

    // 监听 indicator:volume 事件——麦克风和 SDR 均通过此事件推送信号强度
    const unlistenVolume = listen<number>("indicator:volume", (event) => {
      const vol = event.payload;
      sdrSignalRef.current = vol;
      // 如果正在录音且还未启动 SDR 波形，自动切换
      if (currentlyRecording && !rafRef.current) {
        stopWaveform();
        startSdrWaveform();
      } else if (currentlyRecording && !sdrModeRef.current) {
        // 当前在麦克风波形模式，切换到 SDR 模式
        stopWaveform();
        startSdrWaveform();
      }
    });

    // 记录当前是否处于录音状态（用于处理重复事件）
    let currentlyRecording = false;

    const unlistenStatus = listen<StatusMessage>("indicator:status", (event) => {
      const newStatus = event.payload.status;
      const nowRecording = newStatus === "recording" || newStatus === "freetalk";

      setStatus(newStatus);

      if (nowRecording && !currentlyRecording) {
        currentlyRecording = true;
        startTimer();
        // SDR 模式检测：如果已有信号回调在推送，用 SDR 波形；否则用麦克风
        if (sdrSignalRef.current > 0 || sdrModeRef.current) {
          startSdrWaveform();
        } else {
          startWaveform();
        }
      } else if (nowRecording && currentlyRecording) {
        if (!timerRef.current) startTimer();
        if (!rafRef.current) {
          if (sdrModeRef.current) startSdrWaveform(); else startWaveform();
        }
      } else if (!nowRecording && currentlyRecording) {
        currentlyRecording = false;
        stopTimer();
        stopWaveform();
      }

      // processing 状态启动计时
      if (newStatus === "processing") {
        startProcTimer();
      } else if (newStatus !== "refining") {
        stopProcTimer();
      }
    });

    return () => {
      stopTimer();
      stopProcTimer();
      stopWaveform();
      unlistenStatus.then((f) => f());
      unlistenVolume.then((f) => f());
      unlistenSkin.then((f) => f());
      unsubscribe();
    };
  }, []);

  const showAsRecording = status === "recording" || status === "freetalk";
  const isProcessing = status === "processing";
  const isRefining = status === "refining";
  const isLoading = status === "loading";
  const isDone = status === "done";
  const isError = status === "error" || status === "no_voice" || status === "cancelled" || status === "refine_failed";

  // 格式化计时
  const hh = Math.floor(elapsed / 3600);
  const mm = Math.floor((elapsed % 3600) / 60);
  const ss = elapsed % 60;
  const timerDisplay = `${String(hh).padStart(2,"0")}:${String(mm).padStart(2,"0")}:${String(ss).padStart(2,"0")}`;

  const stateLabel = isLoading ? "模型加载中..."
    : isProcessing ? `识别中 ${procElapsed > 0 ? procElapsed + "s" : ""}`
    : isRefining ? `润色中 ${procElapsed > 0 ? procElapsed + "s" : ""}`
    : isDone ? "识别完成"
    : isError ? (status === "no_voice" ? "未检测到语音" : status === "cancelled" ? "已取消" : status === "refine_failed" ? "润色失败" : "识别出错")
    : null;

  return (
    <div className="ind-bar">

      {/* 左侧：计时器 */}
      <div className={`ind-timer${showAsRecording ? " ind-timer--active" : ""}`}>
        {showAsRecording ? timerDisplay : stateLabel ?? timerDisplay}
      </div>

      {/* 中间：波形 Canvas */}
      <div className="ind-waveform">
        <canvas
          ref={canvasRef}
          className={`ind-wave-canvas${
            showAsRecording ? "" :
            isProcessing || isRefining ? " ind-wave-canvas--processing" :
            " ind-wave-canvas--idle"
          }`}
        />
        {(isProcessing || isRefining) && (
          <div className="ind-proc-bar">
            <div className="ind-proc-track">
              <div className="ind-proc-fill" />
            </div>
          </div>
        )}
      </div>

      {/* 右侧：拖拽区域 */}
      <div className="ind-drag-handle" onMouseDown={handleDragBarMouseDown}>
        <div className="ind-drag-dots">
          {Array.from({length: 6}).map((_,i) => <span key={i} className="ind-drag-dot" />)}
        </div>
      </div>

    </div>
  );
}

export default Indicator;
