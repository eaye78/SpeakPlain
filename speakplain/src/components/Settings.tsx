import { useState, useEffect, useRef } from "react";
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
  Input,
  Modal,
  List,
  Tooltip,
  Badge,
  Radio,
  Slider,
  Alert,
  Steps,
} from "antd";
import {
  KeyOutlined,
  AudioOutlined,
  ThunderboltOutlined,
  ReloadOutlined,
  SkinOutlined,
  RobotOutlined,
  MessageOutlined,
  PlusOutlined,
  EditOutlined,
  DeleteOutlined,
  CheckCircleOutlined,
  ApiOutlined,
  EyeOutlined,
  MacCommandOutlined,
  DisconnectOutlined,
  SignalFilled,
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
const { TextArea } = Input;

// ── 类型定义 ──────────────────────────────────────────────────────────────────

interface Persona {
  id: string;
  name: string;
  description?: string;
  system_prompt: string;
  is_builtin: boolean;
}

interface LlmProviderConfig {
  id: string;
  name: string;
  provider_type: "openai_compatible" | "ollama" | "vllm";
  api_base_url: string;
  api_key: string;
  model_name: string;
  timeout_secs: number;
  max_tokens: number;
  temperature: number;
}

interface LlmConfig {
  llm_enabled: boolean;
  persona_id: string;
  llm_provider_id: string;
}

interface CommandMapping {
  command_text: string;
  key_code: number;
  key_name: string;
  modifier: "None" | "Ctrl" | "Alt" | "Shift";
}

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

const AVAILABLE_KEYS = [
  { code: 0x0D, name: "Enter" },
  { code: 0x1B, name: "Escape" },
  { code: 0x20, name: "Space" },
  { code: 0x08, name: "Backspace" },
  { code: 0x09, name: "Tab" },
  { code: 0x70, name: "F1" },
  { code: 0x71, name: "F2" },
  { code: 0x72, name: "F3" },
  { code: 0x73, name: "F4" },
  { code: 0x74, name: "F5" },
  { code: 0x75, name: "F6" },
  { code: 0x76, name: "F7" },
  { code: 0x77, name: "F8" },
  { code: 0x78, name: "F9" },
  { code: 0x79, name: "F10" },
  { code: 0x7a, name: "F11" },
  { code: 0x7b, name: "F12" },
  { code: 0x41, name: "A" },
  { code: 0x42, name: "B" },
  { code: 0x43, name: "C" },
  { code: 0x44, name: "D" },
  { code: 0x45, name: "E" },
  { code: 0x46, name: "F" },
  { code: 0x47, name: "G" },
  { code: 0x48, name: "H" },
  { code: 0x49, name: "I" },
  { code: 0x4A, name: "J" },
  { code: 0x4B, name: "K" },
  { code: 0x4C, name: "L" },
  { code: 0x4D, name: "M" },
  { code: 0x4E, name: "N" },
  { code: 0x4F, name: "O" },
  { code: 0x50, name: "P" },
  { code: 0x51, name: "Q" },
  { code: 0x52, name: "R" },
  { code: 0x53, name: "S" },
  { code: 0x54, name: "T" },
  { code: 0x55, name: "U" },
  { code: 0x56, name: "V" },
  { code: 0x57, name: "W" },
  { code: 0x58, name: "X" },
  { code: 0x59, name: "Y" },
  { code: 0x5A, name: "Z" },
  { code: 0x30, name: "0" },
  { code: 0x31, name: "1" },
  { code: 0x32, name: "2" },
  { code: 0x33, name: "3" },
  { code: 0x34, name: "4" },
  { code: 0x35, name: "5" },
  { code: 0x36, name: "6" },
  { code: 0x37, name: "7" },
  { code: 0x38, name: "8" },
  { code: 0x39, name: "9" },
];

const MODIFIER_KEYS = [
  { value: "None", label: "无" },
  { value: "Ctrl", label: "Ctrl" },
  { value: "Alt", label: "Alt" },
  { value: "Shift", label: "Shift" },
];

interface ASRModel {
  id: string;
  name: string;
  available: boolean;
}

interface SettingsProps {
  activeTab: "general" | "command" | "llm" | "sdr";
}

// SDR类型定义
interface SdrDeviceInfo {
  index: number;
  name: string;
  tuner: string;
  serial: string;
  is_connected: boolean;
}

interface SdrStatus {
  connected: boolean;
  frequency_mhz: number;
  gain_db: number;
  signal_strength: number;
  streaming: boolean;
  output_device: string;
  demod_mode: DemodMode;
  ppm_correction: number;
  vad_active: boolean;
  mock_mode?: boolean;
  debug_sample_rate: number;
  debug_out_sample_rate: number;
  debug_audio_queue_len: number;
  debug_call_test_mode: boolean;
  diag_audio_rms: number;
  diag_iq_range: number;
  diag_iq_dc_i: number;
  ctcss_tone: number;
  ctcss_threshold: number;
  ctcss_detected: boolean;
  ctcss_strength: number;
}

type InputSource = "microphone" | "sdr";
type DemodMode = "nbfm" | "wbfm" | "am" | "usb" | "lsb";

interface SdrConfig {
  enabled: boolean;
  device_index?: number;
  frequency_mhz: number;
  gain_db: number;
  auto_gain: boolean;
  output_device: string;
  input_source: InputSource;
  demod_mode: DemodMode;
  ppm_correction: number;
  vad_threshold: number;
  ctcss_tone: number;
  ctcss_threshold: number;
}

const DEMOD_OPTIONS: { value: DemodMode; label: string; desc: string }[] = [
  { value: "nbfm", label: "NBFM", desc: "窄带调频（对讲机/业余，推荐）" },
  { value: "wbfm", label: "WBFM", desc: "宽带调频（FM广播）" },
  { value: "am",   label: "AM",   desc: "调幅（航空/短波）" },
  { value: "usb",  label: "USB",  desc: "上边带单边带" },
  { value: "lsb",  label: "LSB",  desc: "下边带单边带" },
];

function Settings({ activeTab }: SettingsProps) {
  const [form] = Form.useForm();
  const [audioDevices, setAudioDevices] = useState<string[]>([]);
  const [skins, setSkins] = useState<SkinListItem[]>([]);
  const [asrModels, setAsrModels] = useState<ASRModel[]>([]);
  const [currentAsrModel, setCurrentAsrModel] = useState<string>("sensevoice");
  const [switchingModel, setSwitchingModel] = useState(false);
  const { config, updateConfig, currentSkinId, setCurrentSkinId } = useAppStore();

  // ── 说人话功能状态 ────────────────────────────────────────────────
  const [llmConfig, setLlmConfig] = useState<LlmConfig>({
    llm_enabled: false, persona_id: "formal", llm_provider_id: "",
  });
  const [personas, setPersonas] = useState<Persona[]>([]);
  const [providers, setProviders] = useState<LlmProviderConfig[]>([]);
  const [testingProvider, setTestingProvider] = useState<string | null>(null);

  // 人设编辑弹窗
  const [personaModalOpen, setPersonaModalOpen] = useState(false);
  const [editingPersona, setEditingPersona] = useState<Partial<Persona> | null>(null);
  const [personaForm] = Form.useForm();

  // 人设查看弹窗
  const [viewPersonaOpen, setViewPersonaOpen] = useState(false);
  const [viewingPersona, setViewingPersona] = useState<Persona | null>(null);

  // Provider 编辑弹窗
  const [providerModalOpen, setProviderModalOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<Partial<LlmProviderConfig> | null>(null);
  const [providerForm] = Form.useForm();

  // ── 指令模式状态 ─────────────────────────────────────────────────
  const [commandModeEnabled, setCommandModeEnabled] = useState(false);
  const [commandMappings, setCommandMappings] = useState<CommandMapping[]>([]);
  const [mappingModalOpen, setMappingModalOpen] = useState(false);
  const [editingMapping, setEditingMapping] = useState<Partial<CommandMapping> | null>(null);
  const [mappingForm] = Form.useForm();

  // ── SDR设备状态 ─────────────────────────────────────────────────
  const [sdrDevices, setSdrDevices] = useState<SdrDeviceInfo[]>([]);
  const [sdrAllDevices, setSdrAllDevices] = useState<string[]>([]);
  const [sdrStatus, setSdrStatus] = useState<SdrStatus | null>(null);
  const [sdrConfig, setSdrConfig] = useState<SdrConfig>({
    enabled: false,
    frequency_mhz: 144.5,
    gain_db: 30,
    auto_gain: false,
    output_device: "",
    input_source: "microphone",
    demod_mode: "nbfm",
    ppm_correction: 0,
    vad_threshold: 0.01,
    ctcss_tone: 0,
    ctcss_threshold: 0.15,
  });
  const [sdrLoading, setSdrLoading] = useState(false);
  const [selectedDeviceIndex, setSelectedDeviceIndex] = useState<number | null>(null);
  const [sdrSignal, setSdrSignal] = useState(0);
  const [zadigLoading, setZadigLoading] = useState(false);
  const [callTesting, setCallTesting] = useState(false);
  const [showSdrAdvanced, setShowSdrAdvanced] = useState(false);
  const [lastTestSnapshot, setLastTestSnapshot] = useState<{ status: SdrStatus; signal: number; stoppedAt: Date } | null>(null);
  const [rtlsdrLog, setRtlsdrLog] = useState<string | null>(null);
  const [rtlsdrLogPath, setRtlsdrLogPath] = useState<string>("");
  const sdrSignalTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    loadConfig();
    loadAudioDevices();
    loadASRModels();
    loadLlmData();
    loadCommandMappings();
    loadSdrData();
    skinManager.initialize().then(() => { refreshSkinList(); });
    const unsubscribe = onSkinChange((skin) => { setCurrentSkinId(skin.id); });
    return () => unsubscribe();
  }, []);

  // activeTab 切换到常规设置时同步 input_source（单实例跨Tab共享状态）
  useEffect(() => {
    if (activeTab === "general") {
      invoke<InputSource>("sdr_get_input_source")
        .then((src) => setSdrConfig(prev => ({ ...prev, input_source: src })))
        .catch(() => {});
    }
  }, [activeTab]);

  // ── 指令模式数据加载 ─────────────────────────────────────────────

  const loadCommandMappings = async () => {
    try {
      const [enabled, mappings] = await Promise.all([
        invoke<boolean>("get_command_mode_enabled"),
        invoke<CommandMapping[]>("get_command_mappings"),
      ]);
      setCommandModeEnabled(enabled);
      setCommandMappings(mappings);
    } catch (err) {
      console.error("加载指令映射失败:", err);
    }
  };

  const handleSaveMapping = async () => {
    try {
      const values = await mappingForm.validateFields();
      
      // 根据 key_code 查找对应的 key_name
      const keyInfo = AVAILABLE_KEYS.find(k => k.code === values.key_code);
      if (!keyInfo) {
        message.error("无效的按键选择");
        return;
      }
      
      const mapping: CommandMapping = {
        ...editingMapping,
        ...values,
        key_name: keyInfo.name,
      } as CommandMapping;

      // 校验是否与热键冲突
      const hotkeyVk = form.getFieldValue("hotkey_vk");
      if (mapping.key_code === hotkeyVk) {
        message.error("模拟按键不能与热键设置冲突");
        return;
      }

      await invoke("save_command_mapping", { mapping });
      message.success("保存成功");
      setMappingModalOpen(false);
      loadCommandMappings();
    } catch (err: any) {
      message.error("保存失败: " + err);
    }
  };

  const handleDeleteMapping = async (commandText: string) => {
    try {
      await invoke("delete_command_mapping", { commandText });
      message.success("删除成功");
      loadCommandMappings();
    } catch (err) {
      message.error("删除失败: " + err);
    }
  };

  // ── 说人话数据加载 ───────────────────────────────────────────────

  const loadLlmData = async () => {
    try {
      const [cfg, ps, pvs] = await Promise.all([
        invoke<LlmConfig>("get_llm_config"),
        invoke<Persona[]>("get_personas"),
        invoke<LlmProviderConfig[]>("get_llm_providers"),
      ]);
      // 如果 provider_id 为空但只有一个 provider，自动选中
      if (!cfg.llm_provider_id && pvs.length === 1) {
        await invoke("set_llm_provider", { providerId: pvs[0].id });
        cfg.llm_provider_id = pvs[0].id;
      }
      setLlmConfig(cfg);
      setPersonas(ps);
      setProviders(pvs);
    } catch (err) {
      console.error("加载说人话配置失败:", err);
    }
  };

  const handleLlmEnabledChange = async (enabled: boolean) => {
    try {
      await invoke("set_llm_enabled", { enabled });
      setLlmConfig(prev => ({ ...prev, llm_enabled: enabled }));
    } catch (err) { message.error("设置失败: " + err); }
  };

  const handlePersonaSelect = async (personaId: string) => {
    try {
      await invoke("set_persona", { personaId });
      setLlmConfig(prev => ({ ...prev, persona_id: personaId }));
    } catch (err) { message.error("切换人设失败: " + err); }
  };

  const handleProviderSelect = async (providerId: string) => {
    try {
      await invoke("set_llm_provider", { providerId });
      setLlmConfig(prev => ({ ...prev, llm_provider_id: providerId }));
    } catch (err) { message.error("切换 Provider 失败: " + err); }
  };

  const handleTestProvider = async (provider: LlmProviderConfig) => {
    setTestingProvider(provider.id);
    try {
      const result = await invoke<string>("test_llm_provider", { provider });
      message.success("✓ " + result);
    } catch (err) {
      message.error("✗ 连接失败: " + err);
    } finally {
      setTestingProvider(null);
    }
  };

  const handleSaveProvider = async () => {
    try {
      const values = await providerForm.validateFields();
      const provider: LlmProviderConfig = { ...editingProvider, ...values } as LlmProviderConfig;
      await invoke("save_llm_provider", { provider });
      message.success("已保存");
      setProviderModalOpen(false);
      loadLlmData();
    } catch (err) { message.error("保存失败: " + err); }
  };

  const handleDeleteProvider = async (id: string) => {
    try {
      await invoke("delete_llm_provider", { providerId: id });
      message.success("已删除");
      loadLlmData();
    } catch (err) { message.error("删除失败: " + err); }
  };

  const openNewProvider = async (type: string) => {
    try {
      const defaults = await invoke<LlmProviderConfig>("get_llm_provider_defaults", { providerType: type });
      setEditingProvider(defaults);
      providerForm.setFieldsValue(defaults);
      setProviderModalOpen(true);
    } catch (err) { message.error("初始化失败: " + err); }
  };

  const openEditProvider = (provider: LlmProviderConfig) => {
    setEditingProvider(provider);
    providerForm.setFieldsValue(provider);
    setProviderModalOpen(true);
  };

  const handleSavePersona = async () => {
    try {
      const values = await personaForm.validateFields();
      const autoId = editingPersona?.id || `custom_${Date.now()}`;
      const persona: Persona = { ...editingPersona, ...values, id: autoId, is_builtin: false } as Persona;
      await invoke("save_persona", { persona });
      message.success("已保存");
      setPersonaModalOpen(false);
      loadLlmData();
    } catch (err) { message.error("保存失败: " + err); }
  };

  const handleDeletePersona = async (id: string) => {
    try {
      await invoke("delete_persona", { personaId: id });
      message.success("已删除");
      loadLlmData();
    } catch (err) { message.error("删除失败: " + err); }
  };

  const openNewPersona = () => {
    const newP = { id: "", name: "", description: "", system_prompt: "", is_builtin: false };
    setEditingPersona(newP);
    personaForm.setFieldsValue({ name: "", description: "", system_prompt: "" });
    setPersonaModalOpen(true);
  };

  const openEditPersona = (persona: Persona) => {
    setEditingPersona(persona);
    personaForm.setFieldsValue(persona);
    setPersonaModalOpen(true);
  };


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

  // 自动保存单个设置项
  const handleSettingChange = async (changedValues: any) => {
    try {
      // 获取当前所有表单值
      const currentValues = form.getFieldsValue();
      // 合并变更
      const newValues = { ...currentValues, ...changedValues };
      // 保存到后端
      await invoke("save_config", { newConfig: newValues });
      // 更新全局状态
      updateConfig(newValues);
    } catch (err) {
      message.error("保存失败: " + err);
    }
  };

  // 常规设置内容
  const renderGeneralContent = () => (
    <Space direction="vertical" style={{ width: "100%" }} size="large">
      <Card>
        <Title level={4}><SkinOutlined /> 主题皮肤</Title>
        <Form form={form} layout="vertical" onValuesChange={handleSettingChange}>
          <Form.Item name="skin_id" label="选择皮肤">
            <Select style={{ width: 200 }} value={currentSkinId} onChange={handleSkinChange}>
              {skins.map((skin) => (
                <Select.Option key={skin.id} value={skin.id}>
                  <Space>
                    <span style={{ display: "inline-block", width: 16, height: 16, borderRadius: 4, background: skin.previewColor, marginRight: 8 }} />
                    {skin.name}
                    {skin.isBuiltIn && <Tag>内置</Tag>}
                    {skin.isCustom && <Tag color="blue">自定义</Tag>}
                  </Space>
                </Select.Option>
              ))}
            </Select>
          </Form.Item>
          <Divider />
          <Text type="secondary">将皮肤压缩包(.zip)放入 skins 目录，系统会自动解压并加载</Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}><RobotOutlined /> ASR 模型</Title>
        <Form form={form} layout="vertical" onValuesChange={handleSettingChange}>
          <Form.Item name="asr_model" label="语音识别模型">
            <Select style={{ width: 300 }} value={currentAsrModel} onChange={handleASRModelChange} loading={switchingModel}>
              {asrModels.map((model) => (
                <Select.Option key={model.id} value={model.id} disabled={!model.available}>
                  <Space>
                    {model.name}
                    {!model.available && <Tag color="red">未安装</Tag>}
                    {model.id === currentAsrModel && <Tag color="green">当前</Tag>}
                  </Space>
                </Select.Option>
              ))}
            </Select>
          </Form.Item>
          <Text type="secondary">选择模型后立即加载，加载完成前无法使用语音输入（可能需要数秒）</Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}><KeyOutlined /> 热键设置</Title>
        <Form form={form} layout="vertical" initialValues={config} onValuesChange={handleSettingChange}>
          <Form.Item name="hotkey_vk" label="语音输入热键" rules={[{ required: true }]}>
            <Select style={{ width: 200 }}>
              {HOTKEY_OPTIONS.map((opt) => (<Option key={opt.value} value={opt.value}>{opt.label}</Option>))}
            </Select>
          </Form.Item>
          <Text type="secondary">长按热键开始录音，松手结束；短按切换自由说话模式</Text>
        </Form>
      </Card>

      <Card>
        <Title level={4}><AudioOutlined /> 音频输入</Title>
        <Space direction="vertical" style={{ width: "100%" }}>
          {/* 输入源选择 —— 统一入口 */}
          <div>
            <Text strong style={{ display: "block", marginBottom: 8 }}>语音输入来源</Text>
            <Radio.Group
              value={sdrConfig.input_source}
              onChange={(e) => handleInputSourceChange(e.target.value)}
              buttonStyle="solid"
            >
              <Radio.Button value="microphone">
                <Space><AudioOutlined /> 麦克风</Space>
              </Radio.Button>
              <Radio.Button value="sdr">
                <Space><SignalFilled /> SDR无线电</Space>
              </Radio.Button>
            </Radio.Group>
          </div>

          {/* 麦克风模式：显示设备选择和参数 */}
          {sdrConfig.input_source === "microphone" && (
            <Form form={form} layout="vertical" onValuesChange={handleSettingChange} style={{ marginTop: 4 }}>
              <Form.Item name="audio_device" label="麦克风设备" style={{ marginBottom: 8 }}>
                <Select style={{ width: 300 }} placeholder="使用默认设备" allowClear
                  dropdownRender={(menu) => (
                    <>{menu}<div style={{ padding: "4px 8px", borderTop: "1px solid #f0f0f0" }}>
                      <Button type="link" icon={<ReloadOutlined />} size="small" onClick={loadAudioDevices}>刷新设备列表</Button>
                    </div></>
                  )}>
                  {audioDevices.map((device) => (<Option key={device} value={device}>{device}</Option>))}
                </Select>
              </Form.Item>
              <Form.Item name="silence_timeout_ms" label="静音自动停止 (毫秒)" style={{ marginBottom: 8 }}>
                <InputNumber min={1000} max={10000} step={500} />
              </Form.Item>
              <Form.Item name="vad_threshold" label="语音检测阈值" style={{ marginBottom: 4 }}>
                <InputNumber min={0.001} max={0.5} step={0.001} />
              </Form.Item>
              <Text type="secondary">数值越小越容易检测到语音（默认 0.005）</Text>
            </Form>
          )}

          {/* SDR 模式：引导提示 */}
          {sdrConfig.input_source === "sdr" && (
            <Alert
              message="已切换至 SDR 无线电输入"
              description="热键录音将从 SDR 接收的音频中识别文字。请前往「SDR 设置」页面配置接收频率、解调模式等参数。"
              type="success"
              showIcon
              style={{ marginTop: 4 }}
            />
          )}
        </Space>
      </Card>

      <Card>
        <Title level={4}><ThunderboltOutlined /> 识别设置</Title>
        <Form form={form} layout="vertical" onValuesChange={handleSettingChange}>
          <Form.Item name="use_gpu" valuePropName="checked"><Switch checkedChildren="GPU" unCheckedChildren="CPU" /></Form.Item>
          <Text type="secondary">优先使用 DirectML GPU 加速，不可用时自动回退到 CPU（重启后生效）</Text>
          <Divider />
          <Form.Item name="remove_fillers" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">自动去除"嗯、啊、呃"等语气词</Text>
          <Form.Item name="capitalize_sentences" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">句首字母大写</Text>
          <Form.Item name="optimize_spacing" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">在中英文之间自动添加空格</Text>
          <Form.Item name="restore_clipboard" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">粘贴后恢复原始剪贴板内容</Text>
          <Form.Item name="sound_feedback" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">启用音效反馈</Text>
          <Form.Item name="auto_start" valuePropName="checked"><Switch /></Form.Item>
          <Text type="secondary">开机自动启动</Text>
        </Form>
      </Card>
    </Space>
  );

  // 指令模式内容
  const renderCommandContent = () => (
    <Space direction="vertical" style={{ width: "100%" }} size="large">
              {/* 功能开关 */}
              <Card>
                <Space align="center" style={{ width: "100%", justifyContent: "space-between" }}>
                  <div>
                    <Title level={4} style={{ margin: 0 }}><MacCommandOutlined /> 指令模式</Title>
                    <Text type="secondary">开启后，说出指令文字将触发对应按键而非输入文字</Text>
                  </div>
                  <Switch
                    checked={commandModeEnabled}
                    onChange={async (checked) => {
                      try {
                        await invoke("set_command_mode_enabled", { enabled: checked });
                        setCommandModeEnabled(checked);
                        message.success(checked ? "指令模式已开启" : "指令模式已关闭");
                      } catch (err: any) {
                        message.error("设置失败: " + err);
                      }
                    }}
                    checkedChildren="已开启"
                    unCheckedChildren="已关闭"
                  />
                </Space>
              </Card>

              {/* 指令映射配置 */}
              <Card
                title={<span><KeyOutlined /> 指令映射配置</span>}
                extra={
                  commandModeEnabled && (
                    <Button 
                      type="primary" 
                      icon={<PlusOutlined />}
                      onClick={() => {
                        setEditingMapping({});
                        mappingForm.resetFields();
                        setMappingModalOpen(true);
                      }}
                    >
                      添加映射
                    </Button>
                  )
                }
              >
                {!commandModeEnabled && (
                  <Text type="secondary">请先开启指令模式开关</Text>
                )}
                
                {commandModeEnabled && (
                  <>
                    {commandMappings.length === 0 ? (
                      <Text type="secondary">还没有配置任何指令映射，点击右上角添加</Text>
                    ) : (
                      <List
                        dataSource={commandMappings}
                        renderItem={(item) => (
                          <List.Item
                            style={{
                              borderBottom: "1px solid #f0f0f0",
                              padding: "12px 0",
                            }}
                            actions={[
                              <Button 
                                size="small" 
                                icon={<EditOutlined />}
                                onClick={() => {
                                  setEditingMapping(item);
                                  mappingForm.setFieldsValue(item);
                                  setMappingModalOpen(true);
                                }}
                              >
                                编辑
                              </Button>,
                              <Button 
                                size="small" 
                                danger 
                                icon={<DeleteOutlined />}
                                onClick={() => handleDeleteMapping(item.command_text)}
                              >
                                删除
                              </Button>,
                            ]}
                          >
                            <List.Item.Meta
                              title={
                                <Space size="large">
                                  <Tag color="blue" style={{ fontSize: 14, padding: "4px 12px" }}>
                                    {item.command_text}
                                  </Tag>
                                  <Text type="secondary">→</Text>
                                  <Tag color="green" style={{ fontSize: 14, padding: "4px 12px" }}>
                                    {item.modifier !== "None" ? `${item.modifier} + ` : ""}{item.key_name}
                                  </Tag>
                                </Space>
                              }
                              description={
                                <Text type="secondary" style={{ marginTop: 4, display: "block" }}>
                                  说出"{item.command_text}"将模拟按下 
                                  {item.modifier !== "None" ? `${item.modifier} + ` : ""}{item.key_name} 键
                                </Text>
                              }
                            />
                          </List.Item>
                        )}
                      />
                    )}
                    
                    <Divider />
                    
                    <div style={{ background: "#f6ffed", border: "1px solid #b7eb8f", borderRadius: 4, padding: 12 }}>
                      <Text style={{ color: "#52c41a" }}>💡 使用提示</Text>
                      <ul style={{ margin: "8px 0 0 0", paddingLeft: 20, color: "#666" }}>
                        <li>指令文字必须是一次语音识别的完整结果才会触发</li>
                        <li>如果一段语音中只包含部分指令文字，不会触发指令</li>
                        <li>模拟按键不能与热键设置冲突</li>
                        <li>触发指令时不会进行 LLM 润色，也不会输出文字到光标位置</li>
                      </ul>
                    </div>
                  </>
                )}
              </Card>
            </Space>
          );

  // 说人话内容
  const renderLlmContent = () => (
    <Space direction="vertical" style={{ width: "100%" }} size="large">
      {/* 说人话功能状态提示 */}
      {(() => {
        const enabled = llmConfig.llm_enabled;
        const hasProvider = !!llmConfig.llm_provider_id;
        const currentProvider = providers.find(p => p.id === llmConfig.llm_provider_id);
        const currentPersona = personas.find(p => p.id === llmConfig.persona_id);

        if (!enabled) return null;

        if (!hasProvider) {
          return (
            <Card size="small" style={{ background: "#fff7e6", border: "1px solid #ffd591" }}>
              <Space>
                <Text style={{ color: "#fa8c16" }}>⚠️</Text>
                <Text>请在下方点击选中一个 LLM 提供方，否则说人话功能不生效</Text>
              </Space>
            </Card>
          );
        }

        return (
          <Card size="small" style={{ background: "#f6ffed", border: "1px solid #b7eb8f" }}>
            <Space style={{ width: "100%", justifyContent: "space-between" }}>
              <Space wrap>
                <Text style={{ color: "#52c41a" }}>✅</Text>
                <Text>当前使用：</Text>
                <Text strong>{currentProvider?.name ?? llmConfig.llm_provider_id}</Text>
                <Text type="secondary">({currentProvider?.model_name})</Text>
                <Text type="secondary">· 人设：</Text>
                <Text strong>{currentPersona?.name ?? llmConfig.persona_id}</Text>
              </Space>
              <Button size="small" onClick={loadLlmData}>刷新</Button>
            </Space>
          </Card>
        );
      })()}

      {/* 功能开关 */}
      <Card>
        <Space align="center" style={{ width: "100%", justifyContent: "space-between" }}>
          <div>
            <Title level={4} style={{ margin: 0 }}><MessageOutlined /> 说人话功能</Title>
            <Text type="secondary">语音识别结果经 LLM 润色后输出，支持本地 Ollama、vLLM 及云端 API</Text>
          </div>
          <Switch
            checked={llmConfig.llm_enabled}
            onChange={handleLlmEnabledChange}
            disabled={providers.length === 0}
            checkedChildren="已开启"
            unCheckedChildren="已关闭"
          />
        </Space>
        {providers.length === 0 && (
          <Text type="warning" style={{ display: "block", marginTop: 8 }}>请先配置 LLM 提供方</Text>
        )}
      </Card>

      {/* 人设选择 */}
      <Card>
        <Title level={4}>人设选择</Title>
        <Text type="secondary" style={{ display: "block", marginBottom: 12 }}>当前人设决定 LLM 如何润色你的语音识别结果</Text>
        <List
          dataSource={personas}
          grid={{ gutter: 8, column: 2 }}
          renderItem={(p) => (
            <List.Item style={{ marginBottom: 0 }}>
              <Card
                size="small"
                style={{
                  cursor: "pointer",
                  borderColor: llmConfig.persona_id === p.id ? "#1677ff" : undefined,
                  background: llmConfig.persona_id === p.id ? "#e6f4ff" : undefined,
                  position: "relative",
                }}
                styles={{ body: { padding: "10px 12px" } }}
                onClick={() => handlePersonaSelect(p.id)}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                  <Space align="start" style={{ flex: 1, minWidth: 0 }}>
                    {llmConfig.persona_id === p.id && <CheckCircleOutlined style={{ color: "#1677ff", marginTop: 3, flexShrink: 0 }} />}
                    <div style={{ minWidth: 0 }}>
                      <div><b>{p.name}</b> {p.is_builtin && <Tag style={{ marginLeft: 4 }}>内置</Tag>}</div>
                      {p.description && <Text type="secondary" style={{ fontSize: 12 }}>{p.description}</Text>}
                    </div>
                  </Space>
                  <Space size={4} onClick={(e) => e.stopPropagation()} style={{ flexShrink: 0, marginLeft: 8 }}>
                    <Tooltip title="查看提示词">
                      <Button type="text" size="small" icon={<EyeOutlined />} onClick={() => { setViewingPersona(p); setViewPersonaOpen(true); }} />
                    </Tooltip>
                    {!p.is_builtin && (
                      <>
                        <Tooltip title="编辑">
                          <Button type="text" size="small" icon={<EditOutlined />} onClick={() => openEditPersona(p)} />
                        </Tooltip>
                        <Tooltip title="删除">
                          <Button type="text" size="small" danger icon={<DeleteOutlined />} onClick={() => handleDeletePersona(p.id)} />
                        </Tooltip>
                      </>
                    )}
                  </Space>
                </div>
              </Card>
            </List.Item>
          )}
        />
        <Button icon={<PlusOutlined />} onClick={openNewPersona} style={{ marginTop: 8 }}>新增自定义人设</Button>
      </Card>

      {/* LLM Provider 配置 */}
      <Card
        title={<span><ApiOutlined /> LLM 提供方</span>}
        extra={
          <Select placeholder="新建 Provider" style={{ width: 180 }} onChange={openNewProvider} value={null}>
            <Option value="openai_compatible">☁️ OpenAI 兼容</Option>
            <Option value="ollama">🏠 Ollama (本地)</Option>
            <Option value="vllm">⚡ vLLM (本地/服务器)</Option>
          </Select>
        }
      >
        {providers.length === 0 && (
          <Text type="secondary">还没有配置任何 LLM 提供方，点击右上角新建</Text>
        )}
        <List
          dataSource={providers}
          renderItem={(pv) => (
            <List.Item
              style={{
                cursor: "pointer",
                background: llmConfig.llm_provider_id === pv.id ? "#e6f4ff" : undefined,
                borderLeft: llmConfig.llm_provider_id === pv.id ? "3px solid #1677ff" : "3px solid transparent",
                paddingLeft: 8,
                borderRadius: 4,
              }}
              onClick={() => handleProviderSelect(pv.id)}
              actions={[
                <Button
                  size="small"
                  loading={testingProvider === pv.id}
                  onClick={(e) => { e.stopPropagation(); handleTestProvider(pv); }}
                >测试连接</Button>,
                <Button size="small" icon={<EditOutlined />} onClick={(e) => { e.stopPropagation(); openEditProvider(pv); }} />,
                <Button size="small" danger icon={<DeleteOutlined />} onClick={(e) => { e.stopPropagation(); handleDeleteProvider(pv.id); }} />,
              ]}
            >
              <List.Item.Meta
                title={
                  <Space>
                    <Badge status={llmConfig.llm_provider_id === pv.id ? "processing" : "default"} />
                    <span>{pv.name}</span>
                    {llmConfig.llm_provider_id === pv.id && <Tag color="blue">使用中</Tag>}
                    <Tag>{pv.provider_type}</Tag>
                  </Space>
                }
                description={<Text type="secondary">{pv.api_base_url} · {pv.model_name}</Text>}
              />
            </List.Item>
          )}
        />
      </Card>
    </Space>
  );

  // ── SDR设备数据加载 ─────────────────────────────────────────────

  const loadSdrData = async () => {
    try {
      const [devices, allDevs, status, inputSource] = await Promise.all([
        invoke<SdrDeviceInfo[]>("sdr_get_devices"),
        invoke<string[]>("sdr_get_all_output_devices"),
        invoke<SdrStatus>("sdr_get_status"),
        invoke<InputSource>("sdr_get_input_source"),
      ]);
      setSdrDevices(devices);
      setSdrAllDevices(allDevs);
      setSdrStatus(status);
      setSdrSignal(status.signal_strength);
      // 若只有一台设备且未连接，自动预选
      setSelectedDeviceIndex(prev => {
        if (prev !== null) return prev;
        if (devices.length === 1 && !status.connected) return devices[0].index;
        return null;
      });
      setSdrConfig(prev => ({
        ...prev,
        frequency_mhz: status.frequency_mhz,
        gain_db: status.gain_db,
        output_device: status.output_device,
        demod_mode: status.demod_mode,
        ppm_correction: status.ppm_correction,
        input_source: inputSource,
        ctcss_tone: status.ctcss_tone,
        ctcss_threshold: status.ctcss_threshold,
      }));
    } catch (err) {
      console.error("加载SDR数据失败:", err);
    }
  };

  const handleLaunchZadig = async () => {
    setZadigLoading(true);
    try {
      await invoke("sdr_launch_zadig");
      // Zadig 已退出，自动刷新设备列表
      message.success("驱动安装完成，正在刷新设备列表…");
      await loadSdrData();
    } catch (err: any) {
      const msg = String(err);
      if (msg.includes("取消")) {
        message.info("已取消驱动安装");
      } else {
        message.error("启动 Zadig 失败: " + msg);
      }
    } finally {
      setZadigLoading(false);
    }
  };

  const handleSdrConnect = async (deviceIndex: number) => {
    setSdrLoading(true);
    try {
      await invoke("sdr_connect", { deviceIndex });
      message.success("SDR设备已连接");
      await loadSdrData();
      // 注意：连接成功后不自动启动信号轮询，只有点击"开始运行"后才启动
    } catch (err: any) {
      const errMsg = String(err);
      Modal.error({
        title: "SDR设备连接失败",
        content: (
          <div style={{ whiteSpace: "pre-wrap", marginTop: 8, lineHeight: 1.8 }}>
            {errMsg}
          </div>
        ),
        width: 480,
        okText: "知道了",
      });
    } finally {
      setSdrLoading(false);
    }
  };

  const handleSdrDisconnect = async () => {
    setSdrLoading(true);
    try {
      // 断开前先停止轮询
      if (sdrSignalTimerRef.current) { clearInterval(sdrSignalTimerRef.current); sdrSignalTimerRef.current = null; }
      await invoke("sdr_disconnect");
      message.success("SDR设备已断开");
      setSdrSignal(0);
      await loadSdrData();
    } catch (err: any) {
      message.error("断开失败: " + err);
    } finally {
      setSdrLoading(false);
    }
  };;

  const handleSdrSetFrequency = async (freq: number) => {
    try {
      await invoke("sdr_set_frequency", { freqMhz: freq });
      setSdrConfig(prev => ({ ...prev, frequency_mhz: freq }));
      message.success(`频率已设置为 ${freq} MHz`);
    } catch (err: any) {
      message.error("设置频率失败: " + err);
    }
  };

  const handleSdrSetGain = async (gain: number) => {
    try {
      await invoke("sdr_set_gain", { gainDb: gain });
      setSdrConfig(prev => ({ ...prev, gain_db: gain, auto_gain: false }));
    } catch (err: any) {
      message.error("设置增益失败: " + err);
    }
  };

  const handleSdrSetAutoGain = async (enabled: boolean) => {
    try {
      await invoke("sdr_set_auto_gain", { enabled });
      setSdrConfig(prev => ({ ...prev, auto_gain: enabled }));
    } catch (err: any) {
      message.error("设置自动增益失败: " + err);
    }
  };

  const handleSdrSetDemodMode = async (mode: DemodMode) => {
    try {
      await invoke("sdr_set_demod_mode", { mode });
      setSdrConfig(prev => ({ ...prev, demod_mode: mode }));
      message.success(`解调模式已切换为 ${DEMOD_OPTIONS.find(o => o.value === mode)?.label}`);
    } catch (err: any) {
      message.error("切换解调模式失败: " + err);
    }
  };

  const handleSdrSetPpm = async (ppm: number) => {
    try {
      await invoke("sdr_set_ppm", { ppm });
      setSdrConfig(prev => ({ ...prev, ppm_correction: ppm }));
    } catch (err: any) {
      message.error("设置PPM失败: " + err);
    }
  };

  const handleSdrSetVadThreshold = async (threshold: number) => {
    try {
      await invoke("sdr_set_vad_threshold", { threshold });
      setSdrConfig(prev => ({ ...prev, vad_threshold: threshold }));
    } catch (err: any) {
      message.error("设置VAD阈值失败: " + err);
    }
  };

  const handleSdrSetCtcssTone = async (tone: number) => {
    try {
      await invoke("sdr_set_ctcss_tone", { toneHz: tone });
      setSdrConfig(prev => ({ ...prev, ctcss_tone: tone }));
    } catch (err: any) {
      message.error("设置CTCSS频率失败: " + err);
    }
  };

  const handleSdrSetCtcssThreshold = async (threshold: number) => {
    try {
      await invoke("sdr_set_ctcss_threshold", { threshold });
      setSdrConfig(prev => ({ ...prev, ctcss_threshold: threshold }));
    } catch (err: any) {
      message.error("设置CTCSS门限失败: " + err);
    }
  };

  const handleSdrSetOutputDevice = async (device: string) => {
    try {
      await invoke("sdr_set_output_device", { deviceName: device });
      setSdrConfig(prev => ({ ...prev, output_device: device }));
    } catch (err: any) {
      message.error("设置输出设备失败: " + err);
    }
  };

  const handleSdrStartStream = async () => {
    try {
      await invoke("sdr_start_stream");
      message.success("已开始接收信号");
      await loadSdrData();
      // 启动信号强度轮询
      if (sdrSignalTimerRef.current) clearInterval(sdrSignalTimerRef.current);
      const timer = setInterval(async () => {
        try {
          const [strength, status] = await Promise.all([
            invoke<number>("sdr_get_signal_strength"),
            invoke<SdrStatus>("sdr_get_status"),
          ]);
          setSdrSignal(strength);
          setSdrStatus(status);
        } catch {}
      }, 300);
      sdrSignalTimerRef.current = timer;
    } catch (err: any) {
      message.error("启动接收失败: " + err);
    }
  };

  const handleSdrStopStream = async () => {
    try {
      await invoke("sdr_stop_stream");
      if (sdrSignalTimerRef.current) { clearInterval(sdrSignalTimerRef.current); sdrSignalTimerRef.current = null; }
      message.success("已停止接收信号");
      await loadSdrData();
    } catch (err: any) {
      message.error("停止接收失败: " + err);
    }
  };

  const handleCallTestStart = async () => {
    if (selectedDeviceIndex === null) {
      message.warning("请先选择SDR设备");
      return;
    }
    // 重置上次快照，开始新一轮测试
    setLastTestSnapshot(null);
    setCallTesting(true);
    try {
      await invoke("sdr_call_test_start", { deviceIndex: selectedDeviceIndex });
      message.success("通话测试已开始，请按下手台PTT话务");  
      await loadSdrData();
      // 启动信号强度轮询
      if (sdrSignalTimerRef.current) clearInterval(sdrSignalTimerRef.current);
      const timer = setInterval(async () => {
        try {
          const [strength, status] = await Promise.all([
            invoke<number>("sdr_get_signal_strength"),
            invoke<SdrStatus>("sdr_get_status"),
          ]);
          setSdrSignal(strength);
          setSdrStatus(status);
        } catch {}
      }, 300);
      sdrSignalTimerRef.current = timer;
    } catch (err: any) {
      setCallTesting(false);
      message.error("通话测试失败: " + err);
    }
  };

  const handleCallTestStop = async () => {
    try {
      await invoke("sdr_call_test_stop");
      if (sdrSignalTimerRef.current) { clearInterval(sdrSignalTimerRef.current); sdrSignalTimerRef.current = null; }
      // 保存当前状态快照，供停止后继续展示调试信息
      setSdrStatus(prev => {
        if (prev) setLastTestSnapshot({ status: prev, signal: sdrSignal, stoppedAt: new Date() });
        return prev;
      });
      message.success("通话测试已停止");
      await loadSdrData();
    } catch (err: any) {
      message.error("停止失败: " + err);
    } finally {
      setCallTesting(false);
    }
  };

  const handleViewRtlsdrLog = async () => {
    try {
      const [log, path] = await Promise.all([
        invoke<string>("sdr_get_rtlsdr_log"),
        invoke<string>("sdr_get_rtlsdr_log_path"),
      ]);
      setRtlsdrLog(log || "(日志为空)");
      setRtlsdrLogPath(path);
    } catch (err: any) {
      setRtlsdrLog("读取失败: " + err);
    }
  };

  const handleInputSourceChange = async (source: InputSource) => {
    try {
      await invoke("sdr_set_input_source", { source });
      setSdrConfig(prev => ({ ...prev, input_source: source }));
      message.success(source === "sdr" ? "已切换至SDR语音输入" : "已切换至麦克风输入");
    } catch (err: any) {
      message.error("切换输入源失败: " + err);
    }
  };

  // SDR设置内容
  const renderSdrContent = () => {
    // 计算当前向导步骤（0-based）
    // 步骤0: 连接设备 -> 步骤1: 设置频率(含亚音) -> 步骤2: 运行设备(开始接收) -> 步骤3: 启用语音识别
    const wizardStep = !sdrStatus?.connected ? 0
      : !sdrStatus?.streaming ? 1
      : sdrConfig.input_source !== "sdr" ? 2
      : 3;

    return (
      <Space direction="vertical" style={{ width: "100%" }} size="large">

        {/* 向导步骤条 */}
        <Steps
          current={wizardStep}
          size="small"
          items={[
            {
              title: "连接设备",
              description: sdrStatus?.connected ? `已连接 ${sdrDevices[0]?.tuner || ""}` : "插入RTL-SDR",
              status: sdrStatus?.connected ? "finish" : "process",
            },
            {
              title: "设置频率",
              description: sdrStatus?.connected ? `${sdrConfig.frequency_mhz.toFixed(3)} MHz${sdrConfig.ctcss_tone > 0 ? ` / ${sdrConfig.ctcss_tone}Hz` : ""}` : "连接后可设置",
              status: !sdrStatus?.connected ? "wait" : sdrStatus?.streaming ? "finish" : "process",
            },
            {
              title: "运行设备",
              description: sdrStatus?.streaming ? "✅ 接收信号中" : "启动音频流",
              status: !sdrStatus?.connected ? "wait" : sdrStatus?.streaming ? "finish" : "process",
            },
            {
              title: "启用识别",
              description: sdrConfig.input_source === "sdr" ? "✅ 语音识别中" : "切换语音输入源",
              status: sdrStatus?.streaming && sdrConfig.input_source === "sdr" ? "finish" : sdrStatus?.streaming ? "process" : "wait",
            },
          ]}
        />

        {/* 步骤 1：连接设备 */}
        <Card
          size="small"
          styles={{ header: { background: wizardStep === 0 ? "#e6f7ff" : undefined } }}
          title={
            <Space>
              <Tag color={sdrStatus?.connected ? "success" : "processing"} style={{ margin: 0 }}>
                {sdrStatus?.connected ? "✓" : "1"}
              </Tag>
              <Text strong>连接 SDR 设备</Text>
              {sdrStatus?.connected && <Tag color="green">已完成</Tag>}
            </Space>
          }
        >
          <Space direction="vertical" style={{ width: "100%" }}>
            <Space>
              <Select
                style={{ width: 240 }}
                placeholder="选择SDR设备"
                disabled={sdrStatus?.connected}
                value={sdrStatus?.connected ? sdrDevices.find(d => d.is_connected)?.index : selectedDeviceIndex}
                onChange={(val) => setSelectedDeviceIndex(val)}
              >
                {sdrDevices.map((dev) => (
                  <Select.Option key={dev.index} value={dev.index}>
                    {dev.name} ({dev.tuner}) {dev.serial && `[SN:${dev.serial}]`}
                  </Select.Option>
                ))}
              </Select>
              {!sdrStatus?.connected ? (
                <Button
                  type="primary"
                  onClick={() => selectedDeviceIndex !== null && handleSdrConnect(selectedDeviceIndex)}
                  loading={sdrLoading}
                  disabled={selectedDeviceIndex === null}
                >
                  连接设备
                </Button>
              ) : (
                <Button
                  danger
                  icon={<DisconnectOutlined />}
                  onClick={handleSdrDisconnect}
                  loading={sdrLoading}
                >
                  断开
                </Button>
              )}
              <Button icon={<ReloadOutlined />} size="small" onClick={loadSdrData} />
            </Space>
            {sdrStatus?.connected && (
              <Space direction="vertical" style={{ marginTop: 4, width: "100%" }}>
                {/* 设备信息 */}
                <div style={{ fontSize: 12, color: "#666", marginBottom: 4 }}>
                  {(() => {
                    const connectedDev = sdrDevices.find(d => d.is_connected);
                    if (connectedDev) {
                      return (
                        <span>
                          <b>已连接设备:</b> {connectedDev.name} ({connectedDev.tuner})
                          {connectedDev.serial && (
                            <span style={{ marginLeft: 8, color: "#888", fontFamily: "monospace" }}>
                              SN:{connectedDev.serial}
                            </span>
                          )}
                        </span>
                      );
                    }
                    return <span><b>已连接设备:</b> 设备 #{sdrConfig.device_index ?? 0}</span>;
                  })()}
                </div>
                <Space>
                  <Tag color="blue" style={{ fontVariantNumeric: "tabular-nums", fontSize: 13 }}>
                    📡 {sdrConfig.frequency_mhz.toFixed(3)} MHz
                  </Tag>
                  <Tag
                    color={sdrSignal > 0.6 ? "red" : sdrSignal > 0.2 ? "orange" : sdrSignal > 0.05 ? "green" : "default"}
                    style={{ fontVariantNumeric: "tabular-nums", fontSize: 13 }}
                  >
                    信号 {sdrStatus.connected ? `${Math.min(100, Math.round(sdrSignal * 100))}%` : "—"}
                  </Tag>
                  {sdrConfig.ctcss_tone > 0 && (
                    <Tag color="cyan" style={{ fontSize: 12 }}>
                      🔇 {sdrConfig.ctcss_tone}Hz
                    </Tag>
                  )}
                  {sdrStatus.connected && (
                    <Badge
                      status={sdrStatus.vad_active ? "processing" : "default"}
                      text={<Text style={{ fontSize: 12 }}>{sdrStatus.vad_active ? "检测到语音" : "静音"}</Text>}
                    />
                  )}
                </Space>
              </Space>
            )}
            {sdrDevices.length === 0 && (
              <Alert
                message="未检测到 SDR 设备"
                description={
                  <div style={{ lineHeight: 1.8 }}>
                    <div style={{ marginBottom: 8 }}>
                      Windows 默认驱动不兼容，需要替换为 WinUSB 才能识别设备。点击下方按钮自动完成安装（仅首次需要）。
                    </div>
                    <Button
                      type="primary"
                      loading={zadigLoading}
                      onClick={handleLaunchZadig}
                      style={{ marginBottom: 8 }}
                    >
                      {zadigLoading ? "正在安装驱动…安装完成后请点击 Zadig 中的 Install Driver" : "一键安装 WinUSB 驱动"}
                    </Button>
                    <div style={{ fontSize: 12, color: "#888" }}>
                      将弹出管理员权限确认和 Zadig 界面。在 Zadig 中找到设备，选择 <b>WinUSB</b>，点击 <b>Install Driver</b>，完成后关闭 Zadig 即可。
                    </div>
                    <div style={{ fontSize: 12, color: "#aaa", marginTop: 6 }}>
                      手动下载：
                      <a href="https://zadig.akeo.ie/downloads/" target="_blank" rel="noreferrer">Zadig 官网</a>
                      {" · "}
                      <a href="http://github.com/rtlsdrblog/rtl-sdr-blog/releases/latest/download/Release.zip" target="_blank" rel="noreferrer">RTL-SDR 驱动</a>
                    </div>
                  </div>
                }
                type="warning"
                showIcon
              />
            )}
          </Space>
        </Card>

        {/* 步骤 2：设置频率（含亚音） */}
        <Card
          size="small"
          styles={{ header: { background: wizardStep === 1 ? "#e6f7ff" : undefined } }}
          title={
            <Space>
              <Tag color={sdrStatus?.streaming ? "success" : sdrStatus?.connected ? "processing" : "default"} style={{ margin: 0 }}>
                {sdrStatus?.streaming ? "✓" : "2"}
              </Tag>
              <Text strong>设置接收频率</Text>
              {sdrStatus?.connected && !sdrStatus?.streaming && <Tag color="blue">请操作</Tag>}
              {sdrStatus?.streaming && <Tag color="green">已完成</Tag>}
            </Space>
          }
        >
          <Space direction="vertical" style={{ width: "100%" }}>
            <Space wrap>
              <InputNumber
                min={22}
                max={1100}
                step={0.001}
                precision={3}
                value={sdrConfig.frequency_mhz}
                onChange={(val) => val && handleSdrSetFrequency(val)}
                addonAfter="MHz"
                style={{ width: 180 }}
                disabled={!sdrStatus?.connected}
              />
              <Button
                onClick={() => handleSdrSetFrequency(144.500)}
                disabled={!sdrStatus?.connected}
                size="small"
              >
                144.500 MHz
              </Button>
              <Button
                onClick={() => handleSdrSetFrequency(430.000)}
                disabled={!sdrStatus?.connected}
                size="small"
              >
                430.000 MHz
              </Button>
              <Button
                onClick={() => handleSdrSetFrequency(438.625)}
                disabled={!sdrStatus?.connected}
                size="small"
              >
                438.625 MHz
              </Button>
            </Space>
            <Text type="secondary">将频率设置为与手台相同，常见对讲机频段：144MHz / 430MHz / 438.625MHz</Text>
          </Space>
        </Card>

        {/* CTCSS 亚音设置 */}
        <Card size="small">
          <Title level={5} style={{ marginTop: 0 }}>CTCSS 亚音设置</Title>
          <Space direction="vertical" style={{ width: "100%" }}>
            <Space align="center">
              <Text style={{ width: 100 }}>亚音频率:</Text>
              <Select
                style={{ width: 150 }}
                value={sdrConfig.ctcss_tone === 0 ? "none" : sdrConfig.ctcss_tone}
                onChange={(val) => handleSdrSetCtcssTone(val === "none" ? 0 : parseFloat(val as string))}
                disabled={!sdrStatus?.connected}
              >
                <Select.Option value="none">不使用</Select.Option>
                <Select.Option value={67.0}>67.0 Hz</Select.Option>
                <Select.Option value={71.9}>71.9 Hz</Select.Option>
                <Select.Option value={74.4}>74.4 Hz</Select.Option>
                <Select.Option value={77.0}>77.0 Hz</Select.Option>
                <Select.Option value={79.7}>79.7 Hz</Select.Option>
                <Select.Option value={82.5}>82.5 Hz</Select.Option>
                <Select.Option value={85.4}>85.4 Hz</Select.Option>
                <Select.Option value={88.5}>88.5 Hz</Select.Option>
                <Select.Option value={91.5}>91.5 Hz</Select.Option>
                <Select.Option value={94.8}>94.8 Hz</Select.Option>
                <Select.Option value={97.4}>97.4 Hz</Select.Option>
                <Select.Option value={100.0}>100.0 Hz</Select.Option>
                <Select.Option value={103.5}>103.5 Hz</Select.Option>
                <Select.Option value={107.2}>107.2 Hz</Select.Option>
                <Select.Option value={110.9}>110.9 Hz</Select.Option>
                <Select.Option value={114.8}>114.8 Hz</Select.Option>
                <Select.Option value={118.8}>118.8 Hz</Select.Option>
                <Select.Option value={123.0}>123.0 Hz</Select.Option>
                <Select.Option value={127.3}>127.3 Hz</Select.Option>
                <Select.Option value={131.8}>131.8 Hz</Select.Option>
                <Select.Option value={136.5}>136.5 Hz</Select.Option>
                <Select.Option value={141.3}>141.3 Hz</Select.Option>
                <Select.Option value={146.2}>146.2 Hz</Select.Option>
                <Select.Option value={151.4}>151.4 Hz</Select.Option>
                <Select.Option value={156.7}>156.7 Hz</Select.Option>
                <Select.Option value={162.2}>162.2 Hz</Select.Option>
                <Select.Option value={167.9}>167.9 Hz</Select.Option>
                <Select.Option value={173.8}>173.8 Hz</Select.Option>
                <Select.Option value={179.9}>179.9 Hz</Select.Option>
                <Select.Option value={186.2}>186.2 Hz</Select.Option>
                <Select.Option value={192.8}>192.8 Hz</Select.Option>
                <Select.Option value={203.5}>203.5 Hz</Select.Option>
                <Select.Option value={210.7}>210.7 Hz</Select.Option>
                <Select.Option value={218.1}>218.1 Hz</Select.Option>
                <Select.Option value={225.7}>225.7 Hz</Select.Option>
                <Select.Option value={233.6}>233.6 Hz</Select.Option>
                <Select.Option value={241.8}>241.8 Hz</Select.Option>
                <Select.Option value={250.3}>250.3 Hz</Select.Option>
              </Select>
              <Text type="secondary">仅接收带有此亚音的信号，过滤干扰</Text>
            </Space>
            {sdrConfig.ctcss_tone > 0 && (
              <>
                <Space align="center">
                  <Text style={{ width: 100 }}>检测门限:</Text>
                  <Slider
                    min={0.05}
                    max={0.5}
                    step={0.05}
                    value={sdrConfig.ctcss_threshold}
                    onChange={handleSdrSetCtcssThreshold}
                    style={{ width: 200 }}
                  />
                  <Text>{sdrConfig.ctcss_threshold.toFixed(2)}</Text>
                </Space>
                {sdrStatus?.ctcss_detected !== undefined && (
                  <div style={{ fontSize: 12, color: sdrStatus.ctcss_detected ? "green" : "#999" }}>
                    CTCSS 检测状态: {sdrStatus.ctcss_detected ? "✅ 检测到亚音信号" : "⏳ 等待亚音信号..."}
                    {sdrStatus.ctcss_strength > 0 && ` (强度: ${(sdrStatus.ctcss_strength * 100).toFixed(1)}%)`}
                  </div>
                )}
              </>
            )}
            <Text type="secondary">CTCSS（连续语音控制静噪系统）用于在同一频率上区分不同用户组，只有发送方和接收方使用相同亚音频率时才能通信</Text>
          </Space>
        </Card>

        {/* 步骤 3：运行设备（开始接收信号） */}
        <Card
          size="small"
          styles={{ header: { background: wizardStep === 2 ? "#e6f7ff" : undefined } }}
          title={
            <Space>
              <Tag color={sdrStatus?.streaming ? "success" : sdrStatus?.connected ? "processing" : "default"} style={{ margin: 0 }}>
                {sdrStatus?.streaming ? "✓" : "3"}
              </Tag>
              <Text strong>运行设备</Text>
              {sdrStatus?.connected && !sdrStatus?.streaming && <Tag color="blue">请操作</Tag>}
              {sdrStatus?.streaming && <Tag color="green">运行中</Tag>}
            </Space>
          }
        >
          <Space direction="vertical" style={{ width: "100%" }}>
            {!callTesting ? (
              <>
                <Text type="secondary">启动设备开始接收信号。音频通过选中设备播放，不触发语音识别。用于验证频率和信号是否正常。</Text>
                <Button
                  type="primary"
                  icon={<AudioOutlined />}
                  onClick={handleCallTestStart}
                  disabled={!sdrStatus?.connected}
                  block
                >
                  开始运行
                </Button>
                {lastTestSnapshot && (
                  <div style={{ marginTop: 4 }}>
                    <Text type="secondary" style={{ fontSize: 11 }}>
                      上次运行停止于 {lastTestSnapshot.stoppedAt.toLocaleTimeString()}
                    </Text>
                    <div style={{ fontFamily: "monospace", fontSize: 11, background: "#f0f0f0", padding: "6px 8px", borderRadius: 4, lineHeight: 1.8, marginTop: 4 }}>
                      <div>📡 IQ采样率: {lastTestSnapshot.status.debug_sample_rate?.toLocaleString() ?? "—"} Hz</div>
                      <div>🔊 音频输出采样率: {lastTestSnapshot.status.debug_out_sample_rate?.toLocaleString() ?? "—"} Hz</div>
                      <div>📊 最后队列长度: {lastTestSnapshot.status.debug_audio_queue_len ?? "—"} 样本</div>
                      <div>🔧 解调模式: {lastTestSnapshot.status.demod_mode?.toUpperCase() ?? "—"} | PPM: {lastTestSnapshot.status.ppm_correction ?? 0}</div>
                      <div>💻 增益: {lastTestSnapshot.status.gain_db ?? "—"} dB | 最后信号强度: {Math.min(100, Math.round(lastTestSnapshot.signal * 100))}%</div>
                    </div>
                  </div>
                )}
              </>
            ) : (
              <>
                <Alert
                  message="设备运行中"
                  description={
                    <Space direction="vertical" size={4} style={{ width: "100%" }}>
                      <span>请按下配对手台的 PTT 键进行说话，音频将通过【{sdrConfig.output_device || "默认输出设备"}】播放。</span>
                      <Space wrap>
                        <Badge status={sdrStatus?.vad_active ? "processing" : "default"} />
                        <Text style={{ fontSize: 12 }}>{sdrStatus?.vad_active ? "正在接收信号..." : "等待信号"}</Text>
                        <Text style={{ fontSize: 12 }}>|信号强度：{Math.min(100, Math.round(sdrSignal * 100))}%</Text>
                      </Space>
                      <div style={{ fontFamily: "monospace", fontSize: 11, background: "#f5f5f5", padding: "6px 8px", borderRadius: 4, lineHeight: 1.8 }}>
                        <div>📡 <b>接收频率:</b> {sdrStatus?.frequency_mhz?.toFixed(3) ?? "—"} MHz | IQ采样率: {sdrStatus?.debug_sample_rate?.toLocaleString() ?? "—"} Hz</div>
                        <div>🔇 <b>CTCSS亚音:</b> {sdrStatus?.ctcss_tone && sdrStatus.ctcss_tone > 0 ? (
                          <>
                            {sdrStatus.ctcss_tone.toFixed(1)} Hz 
                            <span style={{marginLeft: 8, color: sdrStatus.ctcss_detected ? 'green' : 'orange'}}>
                              {sdrStatus.ctcss_detected ? `✅已检测 (强度${sdrStatus.ctcss_strength.toFixed(2)})` : '⏳检测中...'}
                            </span>
                          </>
                        ) : "未启用"}</div>
                        <div>🔊 音频输出采样率: {sdrStatus?.debug_out_sample_rate?.toLocaleString() ?? "—"} Hz | 队列长度: {sdrStatus?.debug_audio_queue_len ?? "—"} 样本</div>
                        <div>🔧 解调模式: {sdrStatus?.demod_mode?.toUpperCase() ?? "—"} | PPM: {sdrStatus?.ppm_correction ?? 0} | VAD阈值: {sdrConfig.vad_threshold.toFixed(3)}</div>
                        <div>💻 增益: {sdrStatus?.gain_db ?? "—"} dB | 音频流: {sdrStatus?.streaming ? "运行中" : "已停止"}</div>
                        <div style={{borderTop: "1px solid #ddd", marginTop: 4, paddingTop: 4}}><b>🔬 DSP诊断</b></div>
                        <div>IQ幅度范围: <b>{sdrStatus?.diag_iq_range?.toFixed(4) ?? "—"}</b>
                          {" "}<span style={{color: (sdrStatus?.diag_iq_range ?? 0) > 0.05 ? "green" : (sdrStatus?.diag_iq_range ?? 0) > 0.01 ? "orange" : "red"}}>
                            {(sdrStatus?.diag_iq_range ?? 0) > 0.1 ? "✅信号强" : (sdrStatus?.diag_iq_range ?? 0) > 0.02 ? "⚠️信号弱" : "❌无信号(频率偏差?)"}
                          </span>
                        </div>
                        <div>IQ直流偏置(I): <b>{sdrStatus?.diag_iq_dc_i?.toFixed(4) ?? "—"}</b>
                          {" "}<span style={{color: Math.abs(sdrStatus?.diag_iq_dc_i ?? 0) < 0.05 ? "green" : "orange"}}>
                            {Math.abs(sdrStatus?.diag_iq_dc_i ?? 0) < 0.05 ? "✅正常" : "⚠️偏置大(需调PPM)"}
                          </span>
                        </div>
                        <div>解调音频RMS: <b>{sdrStatus?.diag_audio_rms?.toFixed(5) ?? "—"}</b>
                          {" "}<span style={{color: (sdrStatus?.diag_audio_rms ?? 0) > 0.005 ? "green" : (sdrStatus?.diag_audio_rms ?? 0) > 0.001 ? "orange" : "red"}}>
                            {(sdrStatus?.diag_audio_rms ?? 0) > 0.01 ? "✅音频正常" : (sdrStatus?.diag_audio_rms ?? 0) > 0.001 ? "⚠️音频弱" : "❌音频异常(解调失败?)"}
                          </span>
                        </div>
                      </div>
                    </Space>
                  }
                  type="info"
                  showIcon
                />
                <Button
                  danger
                  icon={<DisconnectOutlined />}
                  onClick={handleCallTestStop}
                  block
                >
                  结束运行
                </Button>
              </>
            )}

            {/* rtl_sdr 日志 */}
            <div>
              <Button size="small" onClick={handleViewRtlsdrLog}>
                查看 rtl_sdr 进程日志
              </Button>
              {rtlsdrLogPath && <Text type="secondary" style={{ fontSize: 10, marginLeft: 8 }}>{rtlsdrLogPath}</Text>}
              {rtlsdrLog !== null && (
                <div style={{ marginTop: 6, fontFamily: "monospace", fontSize: 10, background: "#1a1a1a", color: "#0f0", padding: "8px", borderRadius: 4, maxHeight: 200, overflowY: "auto", whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
                  {rtlsdrLog || "(日志为空，尚未连接设备)"}
                </div>
              )}
            </div>
          </Space>
        </Card>

        {/* 步骤 4：启用语音识别 */}
        <Card
          size="small"
          styles={{ header: { background: wizardStep === 3 ? "#f6ffed" : undefined } }}
          title={
            <Space>
              <Tag color={sdrConfig.input_source === "sdr" ? "success" : "default"} style={{ margin: 0 }}>
                {sdrConfig.input_source === "sdr" ? "✓" : "4"}
              </Tag>
              <Text strong>启用 SDR 语音识别</Text>
              {sdrStatus?.streaming && sdrConfig.input_source !== "sdr" && <Tag color="blue">请操作</Tag>}
              {sdrConfig.input_source === "sdr" && <Tag color="green">识别中</Tag>}
            </Space>
          }
        >
          {sdrConfig.input_source !== "sdr" ? (
            <Space direction="vertical" style={{ width: "100%" }}>
              <Text type="secondary">切换语音输入源为 SDR，设备接收信号后自动识别并输出文字。热键在 SDR 模式下不起作用。</Text>
              <Button
                type="primary"
                icon={<SignalFilled />}
                onClick={() => handleInputSourceChange("sdr")}
                disabled={!sdrStatus?.streaming}
              >
                切换为 SDR 语音输入
              </Button>
            </Space>
          ) : (
            <Space direction="vertical" style={{ width: "100%" }}>
              <Alert
                message="SDR 语音识别已启用"
                description="按手台 PTT 说话，SDR 接收到信号后自动识别并输出文字到光标位置。"
                type="success"
                showIcon
              />
              <Button
                icon={<AudioOutlined />}
                onClick={() => handleInputSourceChange("microphone")}
                size="small"
              >
                切换回麦克风
              </Button>
            </Space>
          )}
        </Card>

        {/* 高级设置折叠区 */}
        <div>
          <Button
            type="link"
            style={{ padding: 0, fontSize: 13 }}
            onClick={() => setShowSdrAdvanced(v => !v)}
          >
            {showSdrAdvanced ? "▲ 收起高级设置" : "▼ 展开高级设置"}
          </Button>
        </div>

        {showSdrAdvanced && (
          <Space direction="vertical" style={{ width: "100%" }} size="middle">
            {/* 增益设置 */}
            <Card size="small">
              <Title level={5} style={{ marginTop: 0 }}>增益设置</Title>
              <Space direction="vertical" style={{ width: "100%" }}>
                <Switch
                  checked={sdrConfig.auto_gain}
                  onChange={handleSdrSetAutoGain}
                  disabled={!sdrStatus?.connected}
                  checkedChildren="自动增益"
                  unCheckedChildren="手动增益"
                />
                {!sdrConfig.auto_gain && (
                  <Space>
                    <Text>增益:</Text>
                    <Slider
                      min={0}
                      max={40}
                      value={sdrConfig.gain_db}
                      onChange={handleSdrSetGain}
                      disabled={!sdrStatus?.connected}
                      style={{ width: 200 }}
                    />
                    <Text>{sdrConfig.gain_db} dB</Text>
                  </Space>
                )}
                <Text type="secondary">
                  增益控制接收信号的放大程度。增益过低信号微弱难以识别；增益过高会引入噪声导致误触发。建议从自动增益开始，信号不稳定时再手动调整（典型值 20–35 dB）。
                </Text>
              </Space>
            </Card>

            {/* 虚拟音频输出设备 */}
            <Card size="small">
              <Title level={5} style={{ marginTop: 0 }}>音频输出设备</Title>
              <Space direction="vertical" style={{ width: "100%" }}>
                <Select
                  style={{ width: 300 }}
                  placeholder="选择音频输出设备（可选）"
                  value={sdrConfig.output_device || undefined}
                  onChange={handleSdrSetOutputDevice}
                  allowClear
                  dropdownRender={(menu) => (
                    <>
                      {menu}
                      <div style={{ padding: "4px 8px", borderTop: "1px solid #f0f0f0" }}>
                        <Button type="link" icon={<ReloadOutlined />} size="small" onClick={loadSdrData}>
                          刷新设备列表
                        </Button>
                      </div>
                    </>
                  )}
                >
                  {sdrAllDevices.map((device) => (
                    <Select.Option key={device} value={device}>
                      {device}
                    </Select.Option>
                  ))}
                </Select>
                <Text type="secondary">SDR解调音频同步输出至此设备（可用于监听），不影响ASR识别</Text>
              </Space>
            </Card>

            {/* 解调模式 */}
            <Card size="small">
              <Title level={5} style={{ marginTop: 0 }}>解调模式</Title>
              <Space direction="vertical" style={{ width: "100%" }}>
                <Radio.Group
                  value={sdrConfig.demod_mode}
                  onChange={(e) => handleSdrSetDemodMode(e.target.value)}
                  buttonStyle="solid"
                >
                  {DEMOD_OPTIONS.map(opt => (
                    <Tooltip key={opt.value} title={opt.desc}>
                      <Radio.Button value={opt.value}>{opt.label}</Radio.Button>
                    </Tooltip>
                  ))}
                </Radio.Group>
                <Text type="secondary">
                  {DEMOD_OPTIONS.find(o => o.value === sdrConfig.demod_mode)?.desc}
                </Text>
              </Space>
            </Card>

            {/* PPM校正 + VAD阈值 */}
            <Card size="small">
              <Title level={5} style={{ marginTop: 0 }}>信号校正参数</Title>
              <Space direction="vertical" style={{ width: "100%" }}>
                <Space align="center">
                  <Text style={{ width: 100 }}>PPM频率校正:</Text>
                  <InputNumber
                    min={-50}
                    max={50}
                    step={1}
                    value={sdrConfig.ppm_correction}
                    onChange={(val) => val !== null && handleSdrSetPpm(val)}
                    addonAfter="ppm"
                    style={{ width: 160 }}
                  />
                  <Text type="secondary">补偿晶振误差（默认0，典型值±20）</Text>
                </Space>
                <Space align="center">
                  <Text style={{ width: 100 }}>VAD检测阈值:</Text>
                  <Slider
                    min={0.001}
                    max={0.1}
                    step={0.001}
                    value={sdrConfig.vad_threshold}
                    onChange={handleSdrSetVadThreshold}
                    style={{ width: 200 }}
                  />
                  <Text>{sdrConfig.vad_threshold.toFixed(3)}</Text>
                </Space>
                <Text type="secondary">VAD阈值越小越灵敏，若误触发可适当调大</Text>
              </Space>
            </Card>
          </Space>
        )}
      </Space>
    );
  };

  // 根据 activeTab 渲染对应内容
  const renderContent = () => {
    switch (activeTab) {
      case "general":
        return renderGeneralContent();
      case "command":
        return renderCommandContent();
      case "llm":
        return renderLlmContent();
      case "sdr":
        return renderSdrContent();
      default:
        return renderGeneralContent();
    }
  };

  return (
    <>
      {renderContent()}
      {/* 人设编辑弹窗 */}
      <Modal
        title={editingPersona?.is_builtin ? "预览人设" : (editingPersona?.id ? "编辑人设" : "新增人设")}
        open={personaModalOpen}
        onOk={editingPersona?.is_builtin ? () => setPersonaModalOpen(false) : handleSavePersona}
        onCancel={() => setPersonaModalOpen(false)}
        okText={editingPersona?.is_builtin ? "关闭" : "保存"}
        cancelText="取消"
        width={600}
      >
        <Form form={personaForm} layout="vertical">
          {editingPersona?.id && (
            <Form.Item name="id" label="ID">
              <Input disabled />
            </Form.Item>
          )}
          <Form.Item name="name" label="显示名称" rules={[{ required: true }]}>
            <Input maxLength={20} disabled={editingPersona?.is_builtin} />
          </Form.Item>
          <Form.Item name="description" label="用途描述">
            <Input maxLength={100} disabled={editingPersona?.is_builtin} />
          </Form.Item>
          <Form.Item name="system_prompt" label="System Prompt" rules={[{ required: true }]}>
            <TextArea rows={6} maxLength={2000} showCount disabled={editingPersona?.is_builtin} />
          </Form.Item>
        </Form>
      </Modal>

      {/* 人设查看弹窗 */}
      <Modal
        title={viewingPersona?.name}
        open={viewPersonaOpen}
        onOk={() => setViewPersonaOpen(false)}
        onCancel={() => setViewPersonaOpen(false)}
        okText="关闭"
        cancelButtonProps={{ style: { display: "none" } }}
        width={600}
      >
        {viewingPersona && (
          <Space direction="vertical" style={{ width: "100%" }}>
            {viewingPersona.description && (
              <Text type="secondary">{viewingPersona.description}</Text>
            )}
            <div style={{ marginTop: 8 }}>
              <Text strong style={{ display: "block", marginBottom: 4 }}>System Prompt</Text>
              <TextArea
                value={viewingPersona.system_prompt}
                readOnly
                rows={10}
                style={{ fontFamily: "monospace", fontSize: 13 }}
              />
            </div>
          </Space>
        )}
      </Modal>

      {/* Provider 编辑弹窗 */}
      <Modal
        title={editingProvider?.id ? "编辑 LLM Provider" : "新建 LLM Provider"}
        open={providerModalOpen}
        onOk={handleSaveProvider}
        onCancel={() => setProviderModalOpen(false)}
        okText="保存"
        cancelText="取消"
        width={560}
      >
        <Form form={providerForm} layout="vertical">
          <Form.Item name="name" label="名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="api_base_url" label="API 地址" rules={[{ required: true }]}>
            <Input placeholder="https://api.openai.com/v1" />
          </Form.Item>
          <Form.Item name="api_key" label="API Key">
            <Input.Password placeholder="本地服务可留空" />
          </Form.Item>
          <Form.Item name="model_name" label="模型名称" rules={[{ required: true }]}>
            <Input placeholder="gpt-4o-mini" />
          </Form.Item>
          <Space size="large">
            <Form.Item name="timeout_secs" label="超时秒数">
              <InputNumber min={5} max={300} />
            </Form.Item>
            <Form.Item name="max_tokens" label="最大 Token">
              <InputNumber min={64} max={4096} />
            </Form.Item>
            <Form.Item name="temperature" label="Temperature">
              <InputNumber min={0} max={2} step={0.1} />
            </Form.Item>
          </Space>
        </Form>
      </Modal>

      {/* 指令映射编辑弹窗 */}
      <Modal
        title={editingMapping?.command_text ? "编辑指令映射" : "添加指令映射"}
        open={mappingModalOpen}
        onOk={handleSaveMapping}
        onCancel={() => setMappingModalOpen(false)}
        okText="保存"
        cancelText="取消"
      >
        <Form form={mappingForm} layout="vertical">
          <Form.Item 
            name="command_text" 
            label="指令文字"
            rules={[
              { required: true, message: "请输入指令文字" },
              { max: 10, message: "最多10个字符" },
              { pattern: /^[\u4e00-\u9fa5a-zA-Z0-9]+$/, message: "仅支持中文、英文、数字" },
            ]}
          >
            <Input placeholder="如：发送" disabled={!!editingMapping?.command_text} />
          </Form.Item>
          <Form.Item 
            name="modifier" 
            label="修饰键"
            initialValue="None"
          >
            <Select style={{ width: 120 }}>
              {MODIFIER_KEYS.map((mod) => (
                <Select.Option key={mod.value} value={mod.value}>
                  {mod.label}
                </Select.Option>
              ))}
            </Select>
          </Form.Item>
          <Form.Item 
            name="key_code" 
            label="按键"
            rules={[{ required: true, message: "请选择按键" }]}
          >
            <Select placeholder="选择按键" style={{ width: 150 }}>
              {AVAILABLE_KEYS.map((key) => (
                <Select.Option key={key.code} value={key.code}>
                  {key.name}
                </Select.Option>
              ))}
            </Select>
          </Form.Item>
        </Form>
      </Modal>
    </>
  );
}

export default Settings;
