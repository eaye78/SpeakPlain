import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter, Routes, Route } from "react-router-dom";
import { ConfigProvider, theme } from "antd";
import zhCN from "antd/locale/zh_CN";
import App from "./App";
import Indicator from "./components/Indicator";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        <Route path="/indicator" element={<Indicator />} />
        <Route
          path="*"
          element={
            <ConfigProvider
              locale={zhCN}
              theme={{
                algorithm: theme.defaultAlgorithm,
                token: {
                  colorPrimary: "#1677ff",
                  borderRadius: 6,
                },
              }}
            >
              <App />
            </ConfigProvider>
          }
        />
      </Routes>
    </HashRouter>
  </React.StrictMode>
);
