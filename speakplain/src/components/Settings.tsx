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
  Tabs,
  Input,
  Modal,
  List,
  Tooltip,
  Badge,
} from "antd";
import {
  KeyOutlined,
  AudioOutlined,
  ThunderboltOutlined,
  SaveOutlined,
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


  useEffect(() => {
    loadConfig();
    loadAudioDevices();
    loadASRModels();
    loadLlmData();
    skinManager.initialize().then(() => { refreshSkinList(); });
    const unsubscribe = onSkinChange((skin) => { setCurrentSkinId(skin.id); });
    return () => unsubscribe();
  }, []);

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
    <>
    <Tabs
      defaultActiveKey="general"
      items={[
        {
          key: "general",
          label: <span><ThunderboltOutlined /> 常规设置</span>,
          children: (
            <Space direction="vertical" style={{ width: "100%" }} size="large">
              <Card>
                <Title level={4}><SkinOutlined /> 主题皮肤</Title>
                <Form form={form} layout="vertical">
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
                <Form form={form} layout="vertical">
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
                <Form form={form} layout="vertical" initialValues={config}>
                  <Form.Item name="hotkey_vk" label="语音输入热键" rules={[{ required: true }]}>
                    <Select style={{ width: 200 }}>
                      {HOTKEY_OPTIONS.map((opt) => (<Option key={opt.value} value={opt.value}>{opt.label}</Option>))}
                    </Select>
                  </Form.Item>
                  <Text type="secondary">长按热键开始录音，松手结束；短按切换自由说话模式</Text>
                </Form>
              </Card>

              <Card>
                <Title level={4}><AudioOutlined /> 音频设置</Title>
                <Form form={form} layout="vertical">
                  <Form.Item name="audio_device" label="麦克风设备">
                    <Select style={{ width: 300 }} placeholder="使用默认设备" allowClear
                      dropdownRender={(menu) => (
                        <>{menu}<div style={{ padding: "4px 8px", borderTop: "1px solid #f0f0f0" }}>
                          <Button type="link" icon={<ReloadOutlined />} size="small" onClick={loadAudioDevices}>刷新设备列表</Button>
                        </div></>
                      )}>
                      {audioDevices.map((device) => (<Option key={device} value={device}>{device}</Option>))}
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
                <Title level={4}><ThunderboltOutlined /> 识别设置</Title>
                <Form form={form} layout="vertical">
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

              <div style={{ textAlign: "center" }}>
                <Button type="primary" icon={<SaveOutlined />} size="large" loading={loading} onClick={handleSave}>保存设置</Button>
              </div>
            </Space>
          ),
        },
        {
          key: "llm",
          label: <span><MessageOutlined /> 说人话</span>,
          children: (
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
          ),
        },
      ]}
    />

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
    </>
  );
}

export default Settings;
