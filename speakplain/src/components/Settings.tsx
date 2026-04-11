import { useState, useEffect } from "react";
import {
  Card,
  Form,
  Select,
  Switch,
  Button,
  Divider,
  Typography,
  Space,
  message,
  InputNumber,
  Tag,
} from "antd";
import {
  KeyOutlined,
  AudioOutlined,
  ThunderboltOutlined,
  SaveOutlined,
  ReloadOutlined,
  SkinOutlined,
  RobotOutlined,
} from "@ant-design/icons";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../stores/appStore";
import {
  getSkinList,
  setSkin,
  onSkinChange,
  skinManager,
  type SkinListItem,
} from "../themes";

const { Title, Text } = Typography;
const { Option } = Select;

const HOTKEY_OPTIONS = [
  { value: 0x70, label: "F1" },
  { value: 0x71, label: "F2" },
  { value: 0x72, label: "F3" },
  { value: 0x73, label: "F4" },
  { value: 0x74, label: "F5" },
  { value: 0x75, label: "F6" },
  { value: 0x76, label: "F7" },
  { value: 0x77, label: "F8" },
  { value: 0x78, label: "F9" },
  { value: 0x79, label: "F10" },
  { value: 0x7a, label: "F11" },
  { value: 0x7b, label: "F12" },
];

interface ASRModel {
  id: string;
  name: string;
  available: boolean;
}

function Settings() {
  const [form] = Form.useForm();
  const [loading, setLoading] = useState(false);
  const [audioDevices, setAudioDevices] = useState<string[]>([]);
  const [skins, setSkins] = useState<SkinListItem[]>([]);
  const [asrModels, setAsrModels] = useState<ASRModel[]>([]);
  const [currentAsrModel, setCurrentAsrModel] = useState<string>("sensevoice");
  const [switchingModel, setSwitchingModel] = useState(false);
  const { config, updateConfig, currentSkinId, setCurrentSkinId } = useAppStore();

  useEffect(() => {
    loadConfig();
    loadAudioDevices();
    loadASRModels();
    // 初始化皮肤系统并刷新列表
    skinManager.initialize().then(() => {
      refreshSkinList();
    });
    
    // 监听皮肤变化
    const unsubscribe = onSkinChange((skin) => {
      setCurrentSkinId(skin.id);
    });
    
    return () => unsubscribe();
  }, []);

  const refreshSkinList = () => {
    const skinList = getSkinList();
    setSkins(skinList);
  };

  const handleSkinChange = async (skinId: string) => {
    const success = await setSkin(skinId);
    if (success) {
      setCurrentSkinId(skinId);
      message.success("皮肤已切换");
    } else {
      message.error("切换皮肤失败");
    }
  };

  const loadASRModels = async () => {
    try {
      const models = await invoke<[string, string, boolean][]>("get_available_asr_models");
      setAsrModels(models.map(([id, name, available]) => ({ id, name, available })));
      
      const current = await invoke<[string, string, boolean]>("get_current_asr_model");
      setCurrentAsrModel(current[0]);
    } catch (err) {
      console.error("加载 ASR 模型列表失败:", err);
      // 设置默认模型列表，确保 UI 可以正常显示
      setAsrModels([
        { id: "sensevoice", name: "SenseVoice (阿里通义)", available: true },
        { id: "qwen3-asr", name: "Qwen3-ASR-0.6B (阿里通义千问)", available: false },
      ]);
      setCurrentAsrModel("sensevoice");
    }
  };

  const handleASRModelChange = async (modelId: string) => {
    const modelName = asrModels.find(m => m.id === modelId)?.name ?? modelId;
    setSwitchingModel(true);
    const hide = message.loading(`正在加载 ${modelName}，请稍候...`, 0);
    try {
      await invoke<string>("switch_asr_model", { modelType: modelId });
      setCurrentAsrModel(modelId);
      message.success(`已切换至 ${modelName}`);
      
      // 更新表单中的配置
      form.setFieldsValue({ asr_model: modelId });
    } catch (err: any) {
      message.error(`切换至 ${modelName} 失败: ` + err);
    } finally {
      hide();
      setSwitchingModel(false);
    }
  };

  const loadAudioDevices = async () => {
    try {
      const devices = await invoke<string[]>("list_audio_devices");
      setAudioDevices(devices);
    } catch (_err) {
    }
  };

  const loadConfig = async () => {
    try {
      const savedConfig = await invoke("get_config");
      form.setFieldsValue(savedConfig);
    } catch (_err) {
    }
  };

  const handleSave = async () => {
    setLoading(true);
    try {
      const values = await form.validateFields();
      await invoke("save_config", { newConfig: values });
      updateConfig(values);
      message.success("设置已保存");
    } catch (err) {
      message.error("保存失败: " + err);
    } finally {
      setLoading(false);
    }
  };

  return (
    <Space direction="vertical" style={{ width: "100%" }} size="large">
      <Card>
        <Title level={4}>
          <SkinOutlined /> 主题皮肤
        </Title>
        <Form form={form} layout="vertical">
          <Form.Item name="skin_id" label="选择皮肤">
            <Select
              style={{ width: 200 }}
              value={currentSkinId}
              onChange={handleSkinChange}
            >
              {skins.map((skin) => (
                <Select.Option key={skin.id} value={skin.id}>
                  <Space>
                    <span
                      style={{
                        display: "inline-block",
                        width: 16,
                        height: 16,
                        borderRadius: 4,
                        background: skin.previewColor,
                        marginRight: 8,
                      }}
                    />
                    {skin.name}
                    {skin.isBuiltIn && <Tag>内置</Tag>}
                    {skin.isCustom && <Tag color="blue">自定义</Tag>}
                  </Space>
                </Select.Option>
              ))}
            </Select>
          </Form.Item>

          <Divider />

          <Text type="secondary">
            将皮肤压缩包(.zip)放入 skins 目录，系统会自动解压并加载
          </Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}>
          <RobotOutlined /> ASR 模型
        </Title>
        <Form form={form} layout="vertical">
          <Form.Item name="asr_model" label="语音识别模型">
            <Select
              style={{ width: 300 }}
              value={currentAsrModel}
              onChange={handleASRModelChange}
              loading={switchingModel}
            >
              {asrModels.map((model) => (
                <Select.Option 
                  key={model.id} 
                  value={model.id}
                  disabled={!model.available}
                >
                  <Space>
                    {model.name}
                    {!model.available && <Tag color="red">未安装</Tag>}
                    {model.id === currentAsrModel && <Tag color="green">当前</Tag>}
                  </Space>
                </Select.Option>
              ))}
            </Select>
          </Form.Item>
          <Text type="secondary">
            选择模型后立即加载，加载完成前无法使用语音输入（可能需要数秒）
          </Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}>
          <KeyOutlined /> 热键设置
        </Title>
        <Form form={form} layout="vertical" initialValues={config}>
          <Form.Item
            name="hotkey_vk"
            label="语音输入热键"
            rules={[{ required: true }]}
          >
            <Select style={{ width: 200 }}>
              {HOTKEY_OPTIONS.map((opt) => (
                <Option key={opt.value} value={opt.value}>
                  {opt.label}
                </Option>
              ))}
            </Select>
          </Form.Item>
          <Text type="secondary">
            长按热键开始录音，松手结束；短按切换自由说话模式
          </Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}>
          <AudioOutlined /> 音频设置
        </Title>
        <Form form={form} layout="vertical">
          <Form.Item name="audio_device" label="麦克风设备">
            <Select
              style={{ width: 300 }}
              placeholder="使用默认设备"
              allowClear
              dropdownRender={(menu) => (
                <>
                  {menu}
                  <div style={{ padding: "4px 8px", borderTop: "1px solid #f0f0f0" }}>
                    <Button
                      type="link"
                      icon={<ReloadOutlined />}
                      size="small"
                      onClick={loadAudioDevices}
                    >
                      刷新设备列表
                    </Button>
                  </div>
                </>
              )}
            >
              {audioDevices.map((device) => (
                <Option key={device} value={device}>
                  {device}
                </Option>
              ))}
            </Select>
          </Form.Item>

          <Form.Item name="silence_timeout_ms" label="静音自动停止 (毫秒)">
            <InputNumber min={1000} max={10000} step={500} />
          </Form.Item>

          <Form.Item name="vad_threshold" label="语音检测阈值">
            <InputNumber min={0.001} max={0.5} step={0.001} />
          </Form.Item>
          <Text type="secondary">数值越小越容易检测到语音（默认 0.005）</Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}>
          <ThunderboltOutlined /> 识别设置
        </Title>
        <Form form={form} layout="vertical">
          <Form.Item name="use_gpu" valuePropName="checked">
            <Switch checkedChildren="GPU" unCheckedChildren="CPU" />
          </Form.Item>
          <Text type="secondary">优先使用 DirectML GPU 加速，不可用时自动回退到 CPU（重启后生效）</Text>

          <Divider />

          <Form.Item name="remove_fillers" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">自动去除"嗯、啊、呃"等语气词</Text>

          <Form.Item name="capitalize_sentences" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">句首字母大写</Text>

          <Form.Item name="optimize_spacing" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">在中英文之间自动添加空格</Text>

          <Form.Item name="restore_clipboard" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">粘贴后恢复原始剪贴板内容</Text>

          <Form.Item name="sound_feedback" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">启用音效反馈</Text>

          <Form.Item name="auto_start" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Text type="secondary">开机自动启动</Text>
        </Form>
      </Card>

      <div style={{ textAlign: "center" }}>
        <Button
          type="primary"
          icon={<SaveOutlined />}
          size="large"
          loading={loading}
          onClick={handleSave}
        >
          保存设置
        </Button>
      </div>
    </Space>
  );
}

export default Settings;
