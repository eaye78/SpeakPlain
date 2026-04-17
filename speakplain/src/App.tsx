import { useEffect, useState } from "react";
import { Layout, Tabs, message } from "antd";
import { 
  HistoryOutlined, 
  AudioOutlined,
  ThunderboltOutlined,
  CodeOutlined,
  MessageOutlined,
  RadarChartOutlined,
} from "@ant-design/icons";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Settings from "./components/Settings";
import History from "./components/History";
import { skinManager } from "./themes";

const { Content } = Layout;

function App() {
  const [activeTab, setActiveTab] = useState("general");

  useEffect(() => {
    // 监听 ASR 引擎初始化结果（后端自动初始化，前端只显示状态）
    const unlistenReady = listen<string>("asr:ready", (event) => {
      message.success("语音引擎就绪: " + event.payload, 3);
    });

    const unlistenError = listen<string>("asr:error", (event) => {
      message.error("语音识别引擎初始化失败：" + event.payload, 10);
    });

    // 监听识别完成
    const unlistenComplete = listen<string>("recognition:complete", (_event) => {});

    // 托盘菜单"识别历史"：显示窗口并切换到历史页签
    const unlistenHistory = listen("tray:open_history", async () => {
      const win = getCurrentWindow();
      await win.show();
      await win.setFocus();
      setActiveTab("history");
    });

    // 监听悬浮框请求皮肤数据事件
    const unlistenSkinRequest = listen("indicator:request_skin", async () => {
      console.log('[App] Received skin request from indicator');
      const currentSkinId = skinManager.getCurrentSkinId();
      await skinManager.preloadAndBroadcastSkin(currentSkinId);
    });

    // 应用启动时，等待皮肤管理器初始化完成后，发送皮肤数据给悬浮框
    const initAndBroadcastSkin = async () => {
      await skinManager.initialize();
      const currentSkinId = skinManager.getCurrentSkinId();
      console.log('[App] Initial skin broadcast:', currentSkinId);
      await skinManager.preloadAndBroadcastSkin(currentSkinId);
    };
    initAndBroadcastSkin();

    return () => {
      unlistenReady.then((f) => f());
      unlistenError.then((f) => f());
      unlistenComplete.then((f) => f());
      unlistenHistory.then((f) => f());
      unlistenSkinRequest.then((f) => f());
    };
  }, []);

  const items = [
    {
      key: "general",
      label: (
        <span>
          <ThunderboltOutlined />
          常规设置
        </span>
      ),
    },
    {
      key: "command",
      label: (
        <span>
          <CodeOutlined />
          指令模式
        </span>
      ),
    },
    {
      key: "llm",
      label: (
        <span>
          <MessageOutlined />
          说人话
        </span>
      ),
    },
    {
      key: "sdr",
      label: (
        <span>
          <RadarChartOutlined />
          SDR设备
        </span>
      ),
    },
    {
      key: "history",
      label: (
        <span>
          <HistoryOutlined />
          历史记录
        </span>
      ),
    },
  ];

  return (
    <Layout style={{ minHeight: "100vh", background: "#f5f5f5" }}>
      <Content style={{ padding: 24 }}>
        <div style={{ maxWidth: 800, margin: "0 auto" }}>
          <h1 style={{ textAlign: "center", marginBottom: 24 }}>
            <AudioOutlined style={{ marginRight: 8 }} />
            说人话 - AI语音输入法
          </h1>
          <Tabs
            activeKey={activeTab}
            onChange={setActiveTab}
            items={items}
            type="card"
            renderTabBar={(props, DefaultTabBar) => <DefaultTabBar {...props} />}
            style={{ marginBottom: 0 }}
            tabBarStyle={{ marginBottom: 0 }}
          />
          {/* 单一 Settings 实例，状态跨 Tab 共享 */}
          <div style={{ background: "#fff", padding: 16, borderRadius: "0 0 8px 8px", border: "1px solid #f0f0f0", borderTop: "none" }}>
            {activeTab !== "history" ? (
              <Settings activeTab={activeTab as "general" | "command" | "llm" | "sdr"} />
            ) : (
              <History />
            )}
          </div>
        </div>
      </Content>
    </Layout>
  );
}

export default App;