import { useEffect, useState } from "react";
import {
  Card,
  List,
  Button,
  Typography,
  Space,
  Empty,
  Popconfirm,
  message,
} from "antd";
import {
  DeleteOutlined,
  CopyOutlined,
  ReloadOutlined,
  ClearOutlined,
} from "@ant-design/icons";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../stores/appStore";

const { Text, Paragraph } = Typography;

interface HistoryItem {
  id: number;
  text: string;
  created_at: string;
  duration_sec: number;
  confidence: number;
}

function History() {
  const [loading, setLoading] = useState(false);
  const { history, setHistory, deleteHistoryItem, clearHistory } = useAppStore();

  useEffect(() => {
    loadHistory();
  }, []);

  const loadHistory = async () => {
    setLoading(true);
    try {
      const items = await invoke<HistoryItem[]>("get_history");
      setHistory(items);
    } catch (_err) {
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    message.success("已复制到剪贴板");
  };

  const handleDelete = async (id: number) => {
    try {
      await invoke("delete_history_item", { id });
      deleteHistoryItem(id);
      message.success("已删除");
    } catch (err) {
      message.error("删除失败: " + err);
    }
  };

  const handleClearAll = async () => {
    try {
      await invoke("clear_history");
      clearHistory();
      message.success("已清空所有历史记录");
    } catch (err) {
      message.error("清空失败: " + err);
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    return date.toLocaleString("zh-CN");
  };

  return (
    <Card
      title="识别历史"
      extra={
        <Space>
          <Popconfirm
            title="确认清空"
            description="确定要清空所有历史记录吗？此操作不可恢复。"
            onConfirm={handleClearAll}
            okText="清空"
            cancelText="取消"
            okButtonProps={{ danger: true }}
            disabled={history.length === 0}
          >
            <Button
              danger
              icon={<ClearOutlined />}
              disabled={history.length === 0}
            >
              清空
            </Button>
          </Popconfirm>
          <Button
            icon={<ReloadOutlined />}
            onClick={loadHistory}
            loading={loading}
          >
            刷新
          </Button>
        </Space>
      }
    >
      {history.length === 0 ? (
        <Empty description="暂无识别记录" />
      ) : (
        <List
          loading={loading}
          dataSource={history}
          renderItem={(item) => (
            <List.Item
              actions={[
                <Button
                  key="copy"
                  icon={<CopyOutlined />}
                  onClick={() => handleCopy(item.text)}
                >
                  复制
                </Button>,
                <Popconfirm
                  key="delete"
                  title="确认删除"
                  description="确定要删除这条记录吗？"
                  onConfirm={() => handleDelete(item.id)}
                  okText="删除"
                  cancelText="取消"
                >
                  <Button danger icon={<DeleteOutlined />}>
                    删除
                  </Button>
                </Popconfirm>,
              ]}
            >
              <List.Item.Meta
                title={
                  <Space>
                    <Text>{formatDate(item.created_at)}</Text>
                    {item.duration_sec > 0 && (
                      <Text type="secondary">
                        ({item.duration_sec}秒)
                      </Text>
                    )}
                  </Space>
                }
                description={
                  <Paragraph
                    ellipsis={{ rows: 2, expandable: true, symbol: "展开" }}
                    style={{ maxWidth: 500 }}
                  >
                    {item.text}
                  </Paragraph>
                }
              />
            </List.Item>
          )}
        />
      )}
    </Card>
  );
}

export default History;
