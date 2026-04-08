/**
 * 皮肤管理器
 * 支持文件夹形式的皮肤，每个皮肤包含：
 * - skin.json: 主配置文件
 * - styles.css: 样式文件（可选）
 * - background.png: 背景图片（可选）
 * 
 * 皮肤 id 统一存储在后端 SQLite 数据库
 * 系统启动时动态扫描 skins 目录加载所有可用皮肤
 * 自动解压 skins 目录下的 .zip 皮肤包
 */
import type { SkinConfig, SkinListItem, SkinJson } from "./skinTypes";
import { builtInSkins, getDefaultSkin } from "./defaultSkins";
import { emit, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

// 辅助函数：读取皮肤文件内容（通过后端命令）
const readSkinFile = async (skinId: string, filename: string): Promise<string | null> => {
  try {
    return await invoke<string>("read_skin_file", { skinId, filename });
  } catch (e) {
    console.error(`[SkinManager] Failed to read ${filename} for ${skinId}:`, e);
    return null;
  }
};

// 辅助函数：读取皮肤背景图片为 base64
const readSkinBackground = async (skinId: string): Promise<string | null> => {
  try {
    return await invoke<string>("read_skin_background_base64", { skinId });
  } catch (e) {
    console.error(`[SkinManager] Failed to read background for ${skinId}:`, e);
    return null;
  }
};

// Tauri 跨窗口皮肤广播事件名
const SKIN_CHANGE_EVENT = "skin:change";
const SKIN_DATA_EVENT = "skin:data"; // 主窗口向悬浮框发送完整皮肤数据

// 当前注入的样式元素 ID
const SKIN_STYLE_ID = "skin-dynamic-style";

// 扫描到的皮肤缓存（包括内置和自定义文件夹皮肤）
class SkinManager {
  private availableSkins: Map<string, SkinConfig> = new Map();
  private currentSkin: SkinConfig = getDefaultSkin();
  private listeners: Set<(skin: SkinConfig) => void> = new Set();
  private initialized = false;

  constructor() {
    this.listenCrossWindowSkinChange();
  }

  // 初始化：扫描皮肤文件夹并加载
  async initialize(): Promise<void> {
    if (this.initialized) {
      console.log('[SkinManager] Already initialized');
      return;
    }
    console.log('[SkinManager] Initializing...');
    await this.scanSkinFolders();
    console.log('[SkinManager] Available skins after scan:', Array.from(this.availableSkins.keys()));
    
    // 打印所有皮肤的 hasBackgroundImage 状态
    for (const [id, skin] of this.availableSkins) {
      console.log(`[SkinManager] Skin ${id}: hasBackgroundImage=${skin.hasBackgroundImage}, folderPath=${skin.folderPath}`);
    }
    
    await this.initFromDB();
    this.initialized = true;
    console.log('[SkinManager] Initialized, current skin:', this.currentSkin.id, 'hasBackgroundImage:', this.currentSkin.hasBackgroundImage);
  }

  // 动态扫描 skins 目录下的所有皮肤文件夹
  private async scanSkinFolders(): Promise<void> {
    // 1. 先加载内置皮肤
    for (const skin of builtInSkins) {
      this.availableSkins.set(skin.id, skin);
    }

    // 2. 通过后端命令扫描 skins 目录（自动解压 zip 包）

    
    try {
      const skinIds = await invoke<string[]>("scan_skin_folders");
      console.log(`[SkinManager] Scanned skin folders:`, skinIds);
      for (const skinId of skinIds) {
        const folderSkin = await this.loadSkinFromFolder(skinId);
        if (folderSkin) {
          console.log(`[SkinManager] Loaded skin ${skinId} from folder, hasBackgroundImage:`, folderSkin.hasBackgroundImage);
          // 如果是内置皮肤，用文件夹中的配置完全替换（确保所有配置最新）
          if (this.availableSkins.has(skinId)) {
            this.availableSkins.set(skinId, folderSkin);
          } else {
            // 非内置皮肤（自定义皮肤），直接添加
            this.availableSkins.set(folderSkin.id, folderSkin);
          }
        } else {
          console.log(`[SkinManager] Failed to load skin ${skinId} from folder`);
        }
      }
    } catch (e) {
      console.error('[SkinManager] Error scanning skin folders:', e);
      // 扫描失败则只使用内置皮肤
    }
  }

  // 从皮肤文件夹加载配置
  private async loadSkinFromFolder(skinId: string): Promise<SkinConfig | null> {
    try {
      console.log(`[SkinManager] Loading skin from folder: ${skinId}`);
      // 通过后端命令读取 skin.json
      const configContent = await readSkinFile(skinId, "skin.json");
      console.log(`[SkinManager] Config content for ${skinId}:`, configContent?.substring(0, 200));
      if (!configContent) {
        console.log(`[SkinManager] Failed to load ${skinId}: skin.json not found`);
        return null;
      }
      
      const json: SkinJson = JSON.parse(configContent);
      console.log(`[SkinManager] Parsed ${skinId}, hasBackgroundImage:`, json.hasBackgroundImage);
      return {
        id: json.id,
        name: json.name,
        description: json.description,
        author: json.author,
        version: json.version,
        isBuiltIn: false,
        hasBackgroundImage: json.hasBackgroundImage ?? false,
        folderPath: `skins/${skinId}`,
        colors: {
          background: json.colors.background || "#e8ecf3",
          backgroundGradient: json.colors.backgroundGradient,
          textPrimary: json.colors.textPrimary || "#4a5568",
          textSecondary: json.colors.textSecondary || "#8a9ab8",
          textActive: json.colors.textActive || "#2563eb",
          waveformPrimary: json.colors.waveformPrimary || "#3b6beb",
          waveformSecondary: json.colors.waveformSecondary,
          waveformGradient: json.colors.waveformGradient,
          dragDot: json.colors.dragDot || "#b0bcd0",
          dragDotHover: json.colors.dragDotHover,
          processingDot: json.colors.processingDot || "#3b6beb",
          shadowLight: json.colors.shadowLight || "#ffffff",
          shadowDark: json.colors.shadowDark || "#c5cad4",
        },
        dimensions: {
          borderRadius: json.dimensions?.borderRadius ?? 12,
          paddingX: json.dimensions?.paddingX ?? 14,
          paddingY: json.dimensions?.paddingY ?? 0,
          gap: json.dimensions?.gap ?? 8,
        },
        animations: {
          transitionDuration: json.animations?.transitionDuration || "0.3s",
        },
      };
    } catch (e) {
      console.error(`[SkinManager] Error loading skin ${skinId}:`, e);
      return null;
    }
  }

  // 从数据库加载皮肤 id 并应用
  private async initFromDB(): Promise<void> {
    try {
      const skinId = await invoke<string>("get_skin_id");
      await this.applySkin(skinId);
    } catch {
      this.applySkinToCSS(this.currentSkin);
    }
  }

  // 监听跨窗口皮肤事件
  private listenCrossWindowSkinChange(): void {
    // 监听皮肤切换事件（皮肤ID）
    listen<string>(SKIN_CHANGE_EVENT, (event) => {
      this.applySkin(event.payload);
    }).catch(() => {});
    
    // 监听完整皮肤数据事件（用于悬浮框窗口接收预加载数据）
    listen<{skinId: string, cssText: string | null, backgroundBase64: string | null}>(SKIN_DATA_EVENT, async (event) => {
      const { skinId, cssText, backgroundBase64 } = event.payload;
      console.log(`[SkinManager] Received skin data for ${skinId}, css: ${cssText?.length || 0}, bg: ${backgroundBase64?.length || 0}`);
      
      // 获取或创建皮肤配置
      let skin = this.availableSkins.get(skinId);
      if (!skin) {
        // 如果皮肤不存在，创建一个基本的配置
        skin = {
          id: skinId,
          name: skinId,
          isBuiltIn: false,
          hasBackgroundImage: !!backgroundBase64,
          folderPath: `skins/${skinId}`,
          colors: {
            background: "#e8ecf3",
            textPrimary: "#4a5568",
            textSecondary: "#8a9ab8",
            textActive: "#2563eb",
            waveformPrimary: "#3b6beb",
            dragDot: "#b0bcd0",
            processingDot: "#3b6beb",
            shadowLight: "#ffffff",
            shadowDark: "#c5cad4",
          },
          dimensions: {
            borderRadius: 12,
            paddingX: 14,
            paddingY: 0,
            gap: 8,
          },
          animations: {
            transitionDuration: "0.3s",
          },
        };
        this.availableSkins.set(skinId, skin);
      } else {
        // 更新 hasBackgroundImage 状态
        if (backgroundBase64) {
          skin.hasBackgroundImage = true;
        }
      }
      
      // 直接应用 CSS
      this.applySkinToCSS(skin);
      
      // 注入样式
      if (cssText) {
        this.injectStyle(cssText);
      } else {
        this.removeInjectedStyle();
      }
      
      // 应用背景图片
      if (backgroundBase64 && skin.hasBackgroundImage) {
        document.body.style.backgroundImage = `url(data:image/png;base64,${backgroundBase64})`;
        document.body.classList.add("has-bg-image");
        console.log(`[SkinManager] Applied background image for ${skinId}`);
      } else {
        document.body.style.backgroundImage = "";
        document.body.classList.remove("has-bg-image");
        console.log(`[SkinManager] No background image for ${skinId}, hasBackgroundImage:`, skin.hasBackgroundImage);
      }
      
      this.currentSkin = skin;
      this.notifyListeners();
    }).catch(() => {});
  }
  
  // 预加载皮肤数据并广播给悬浮框（在主窗口调用）
  async preloadAndBroadcastSkin(skinId: string): Promise<void> {
    const skin = this.availableSkins.get(skinId);
    if (!skin) {
      console.log(`[SkinManager] Skin ${skinId} not found in available skins`);
      return;
    }
    
    console.log(`[SkinManager] Preloading skin ${skinId} for broadcast, folderPath:`, skin.folderPath, 'hasBackgroundImage:', skin.hasBackgroundImage);
    
    // 读取样式文件
    let cssText: string | null = null;
    if (skin.folderPath) {
      const folderSkinId = skin.folderPath.replace('skins/', '');
      console.log(`[SkinManager] Reading styles.css for ${folderSkinId}`);
      cssText = await readSkinFile(folderSkinId, "styles.css");
      console.log(`[SkinManager] styles.css result:`, cssText ? `loaded ${cssText.length} chars` : 'null');
    }
    
    // 读取背景图片
    let backgroundBase64: string | null = null;
    if (skin.hasBackgroundImage && skin.folderPath) {
      const folderSkinId = skin.folderPath.replace('skins/', '');
      console.log(`[SkinManager] Reading background.png for ${folderSkinId}`);
      backgroundBase64 = await readSkinBackground(folderSkinId);
      console.log(`[SkinManager] background.png result:`, backgroundBase64 ? `loaded ${backgroundBase64.length} chars` : 'null');
    }
    
    // 广播完整皮肤数据
    console.log(`[SkinManager] Broadcasting skin data for ${skinId}, css:`, cssText?.length || 0, 'bg:', backgroundBase64?.length || 0);
    await emit(SKIN_DATA_EVENT, { skinId, cssText, backgroundBase64 });
  }

  // 获取所有可用皮肤列表
  getSkinList(): SkinListItem[] {
    const list: SkinListItem[] = [];
    for (const skin of this.availableSkins.values()) {
      list.push({
        id: skin.id,
        name: skin.name,
        description: skin.description,
        isBuiltIn: skin.isBuiltIn,
        isCustom: !skin.isBuiltIn,
        previewColor: skin.colors.waveformPrimary,
        hasBackgroundImage: skin.hasBackgroundImage,
      });
    }
    return list;
  }

  getCurrentSkin(): SkinConfig { return this.currentSkin; }
  getCurrentSkinId(): string { return this.currentSkin.id; }

  // 切换皮肤：更新内存 + 保存数据库 + 广播其他窗口
  async setSkin(skinId: string): Promise<boolean> {
    const skin = this.availableSkins.get(skinId);
    if (!skin) return false;
    await this.applySkinInternal(skin);
    invoke("save_skin_id", { skinId }).catch(() => {});
    // 先广播完整皮肤数据，再广播皮肤切换事件
    await this.preloadAndBroadcastSkin(skinId);
    emit(SKIN_CHANGE_EVENT, skinId).catch(() => {});
    return true;
  }

  // 仅应用皮肤到本窗口（不广播，不持久化）
  async applySkin(skinId: string): Promise<void> {
    const skin = this.availableSkins.get(skinId);
    if (skin) {
      await this.applySkinInternal(skin);
    }
  }

  // 内部应用皮肤：CSS 变量 + 样式文件 + 背景图片
  private async applySkinInternal(skin: SkinConfig): Promise<void> {
    this.currentSkin = skin;
    this.applySkinToCSS(skin);
    await this.loadSkinStyles(skin);
    await this.loadSkinBackground(skin);
    this.notifyListeners();
  }

  // 应用皮肤 CSS 变量
  private applySkinToCSS(skin: SkinConfig): void {
    const root = document.documentElement;
    const { colors, dimensions, animations } = skin;
    root.style.setProperty("--skin-bg", colors.background);
    root.style.setProperty("--skin-bg-gradient", colors.backgroundGradient || colors.background);
    root.style.setProperty("--skin-text-primary", colors.textPrimary);
    root.style.setProperty("--skin-text-secondary", colors.textSecondary);
    root.style.setProperty("--skin-text-active", colors.textActive);
    root.style.setProperty("--skin-wave-primary", colors.waveformPrimary);
    root.style.setProperty("--skin-wave-secondary", colors.waveformSecondary || colors.waveformPrimary);
    root.style.setProperty("--skin-wave-gradient", colors.waveformGradient || colors.waveformPrimary);
    root.style.setProperty("--skin-drag-dot", colors.dragDot);
    root.style.setProperty("--skin-drag-dot-hover", colors.dragDotHover || colors.dragDot);
    root.style.setProperty("--skin-processing-dot", colors.processingDot);
    root.style.setProperty("--skin-shadow-light", colors.shadowLight);
    root.style.setProperty("--skin-shadow-dark", colors.shadowDark);
    root.style.setProperty("--skin-border-radius", `${dimensions.borderRadius}px`);
    root.style.setProperty("--skin-padding-x", `${dimensions.paddingX}px`);
    root.style.setProperty("--skin-padding-y", `${dimensions.paddingY}px`);
    root.style.setProperty("--skin-gap", `${dimensions.gap}px`);
    root.style.setProperty("--skin-transition", animations.transitionDuration);
  }

  // 加载皮肤样式文件（通过后端命令读取）
  private async loadSkinStyles(skin: SkinConfig): Promise<void> {
    if (!skin.folderPath) return;
    try {
      const skinId = skin.folderPath.replace('skins/', '');
      const cssText = await readSkinFile(skinId, "styles.css");
      if (cssText) {
        this.injectStyle(cssText);
      } else {
        this.removeInjectedStyle();
      }
    } catch {
      this.removeInjectedStyle();
    }
  }

  // 加载皮肤背景图片
  private async loadSkinBackground(skin: SkinConfig): Promise<void> {
    console.log(`[SkinManager] loadSkinBackground called, hasBackgroundImage:`, skin.hasBackgroundImage, 'folderPath:', skin.folderPath);
    if (!skin.hasBackgroundImage || !skin.folderPath) {
      console.log('[SkinManager] No background image or folderPath, clearing...');
      document.body.style.backgroundImage = "";
      document.body.classList.remove("has-bg-image");
      return;
    }
    try {
      const skinId = skin.folderPath.replace('skins/', '');
      console.log(`[SkinManager] Reading background for ${skinId}`);
      const base64 = await readSkinBackground(skinId);
      console.log(`[SkinManager] Background base64 length:`, base64?.length || 0);
      if (base64) {
        document.body.style.backgroundImage = `url(data:image/png;base64,${base64})`;
        document.body.classList.add("has-bg-image");
        console.log('[SkinManager] Background applied successfully');
      } else {
        document.body.style.backgroundImage = "";
        document.body.classList.remove("has-bg-image");
        console.log('[SkinManager] Background base64 is empty');
      }
    } catch (e) {
      console.error('[SkinManager] Error loading background:', e);
      document.body.style.backgroundImage = "";
      document.body.classList.remove("has-bg-image");
    }
  }

  // 注入样式到页面
  private injectStyle(cssText: string): void {
    this.removeInjectedStyle();
    const styleEl = document.createElement("style");
    styleEl.id = SKIN_STYLE_ID;
    styleEl.textContent = cssText;
    document.head.appendChild(styleEl);
  }

  // 移除注入的样式
  private removeInjectedStyle(): void {
    const existing = document.getElementById(SKIN_STYLE_ID);
    if (existing) {
      existing.remove();
    }
  }

  // 监听皮肤变化
  onSkinChange(callback: (skin: SkinConfig) => void): () => void {
    this.listeners.add(callback);
    return () => { this.listeners.delete(callback); };
  }

  private notifyListeners(): void {
    for (const listener of this.listeners) {
      listener(this.currentSkin);
    }
  }
}

// 单例
export const skinManager = new SkinManager();

// 便捷导出
export const getSkinList = () => skinManager.getSkinList();
export const getCurrentSkin = () => skinManager.getCurrentSkin();
export const setSkin = (skinId: string) => skinManager.setSkin(skinId);
export const onSkinChange = (callback: (skin: SkinConfig) => void) => skinManager.onSkinChange(callback);
