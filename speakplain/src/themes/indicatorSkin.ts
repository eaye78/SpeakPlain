/**
 * 指示器皮肤适配器
 * 为Indicator组件提供皮肤应用功能
 */
import { skinManager } from "./skinManager";
import type { SkinConfig } from "./skinTypes";
import { invoke } from "@tauri-apps/api/core";

// 辅助函数：读取皮肤文件内容（通过后端命令）
const readSkinFile = async (skinId: string, filename: string): Promise<string | null> => {
  try {
    return await invoke<string>("read_skin_file", { skinId, filename });
  } catch (e) {
    console.error(`[Skin] Failed to read ${filename} for ${skinId}:`, e);
    return null;
  }
};

// 辅助函数：读取皮肤背景图片为 base64
const readSkinBackground = async (skinId: string): Promise<string | null> => {
  try {
    return await invoke<string>("read_skin_background_base64", { skinId });
  } catch (e) {
    console.error(`[Skin] Failed to read background for ${skinId}:`, e);
    return null;
  }
};

// 当前注入的样式元素 ID
const SKIN_STYLE_ID = "skin-dynamic-style";

// 加载皮肤资源（样式文件 + 背景图片）
const loadSkinAssets = async (skin: SkinConfig): Promise<void> => {
  await loadSkinStyles(skin);
  await loadSkinBackground(skin);
};

// 加载皮肤样式文件
const loadSkinStyles = async (skin: SkinConfig): Promise<void> => {
  if (!skin.folderPath) {
    console.log('[Skin] No folderPath, skipping styles');
    return;
  }
  try {
    const skinId = skin.folderPath.replace('skins/', '');
    const cssText = await readSkinFile(skinId, "styles.css");
    console.log('[Skin] CSS loaded, length:', cssText?.length || 0);
    if (cssText) {
      injectStyle(cssText);
    } else {
      removeInjectedStyle();
    }
  } catch (e) {
    console.error('[Skin] Error loading styles:', e);
    removeInjectedStyle();
  }
};

// 加载皮肤背景图片
const loadSkinBackground = async (skin: SkinConfig): Promise<void> => {
  console.log('[Skin] Loading background, hasBackgroundImage:', skin.hasBackgroundImage, 'folderPath:', skin.folderPath);
  console.log('[Skin] Body classes before:', document.body.classList.toString());
  
  if (!skin.hasBackgroundImage || !skin.folderPath) {
    console.log('[Skin] No background image, clearing...');
    document.body.style.backgroundImage = "";
    document.body.classList.remove("has-bg-image");
    return;
  }
  
  try {
    const skinId = skin.folderPath.replace('skins/', '');
    const base64 = await readSkinBackground(skinId);
    console.log('[Skin] Background base64 length:', base64?.length || 0);
    
    if (base64) {
      document.body.classList.add("has-bg-image");
      document.body.style.backgroundImage = `url(data:image/png;base64,${base64})`;
    } else {
      document.body.style.backgroundImage = "";
      document.body.classList.remove("has-bg-image");
    }
    
    console.log('[Skin] Body classes after:', document.body.classList.toString());
    console.log('[Skin] Body has indicator-page?', document.body.classList.contains('indicator-page'));
  } catch (e) {
    console.error('[Skin] Error loading background:', e);
    document.body.style.backgroundImage = "";
    document.body.classList.remove("has-bg-image");
  }
};

// 注入样式到页面
const injectStyle = (cssText: string): void => {
  removeInjectedStyle();
  const styleEl = document.createElement("style");
  styleEl.id = SKIN_STYLE_ID;
  styleEl.textContent = cssText;
  document.head.appendChild(styleEl);
};

// 移除注入的样式
const removeInjectedStyle = (): void => {
  const existing = document.getElementById(SKIN_STYLE_ID);
  if (existing) {
    existing.remove();
  }
};

// 获取当前皮肤（指示器专用）
export const getIndicatorSkin = (): SkinConfig => {
  return skinManager.getCurrentSkin();
};

// 监听皮肤变化（指示器专用）
export const onIndicatorSkinChange = (callback: (skin: SkinConfig) => void): (() => void) => {
  return skinManager.onSkinChange(callback);
};

// 应用皮肤到指示器
export const applySkinToIndicator = async (): Promise<void> => {
  const skin = skinManager.getCurrentSkin();
  const { colors, dimensions, animations } = skin;

  const root = document.documentElement;

  // 颜色变量
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

  // 尺寸变量
  root.style.setProperty("--skin-border-radius", `${dimensions.borderRadius}px`);
  root.style.setProperty("--skin-padding-x", `${dimensions.paddingX}px`);
  root.style.setProperty("--skin-padding-y", `${dimensions.paddingY}px`);
  root.style.setProperty("--skin-gap", `${dimensions.gap}px`);

  // 动画变量
  root.style.setProperty("--skin-transition", animations.transitionDuration);

  // 加载皮肤专属样式文件和背景图片（优先于背景色设置）
  await loadSkinAssets(skin);
  
  // 设置body背景色（仅当没有背景图片时）
  if (document.body.classList.contains("indicator-page") && !skin.hasBackgroundImage) {
    document.body.style.background = colors.backgroundGradient || colors.background;
  }
};

// 获取波形绘制颜色
export const getWaveformColor = (): string => {
  const skin = skinManager.getCurrentSkin();
  return skin.colors.waveformGradient || skin.colors.waveformPrimary;
};

// 获取波形渐变（用于Canvas）
export const getWaveformGradient = (ctx: CanvasRenderingContext2D, width: number): CanvasGradient => {
  const skin = skinManager.getCurrentSkin();
  const gradient = ctx.createLinearGradient(0, 0, width, 0);
  
  if (skin.colors.waveformGradient) {
    // 如果定义了渐变，使用渐变
    gradient.addColorStop(0, skin.colors.waveformPrimary);
    gradient.addColorStop(1, skin.colors.waveformSecondary || skin.colors.waveformPrimary);
  } else {
    // 否则使用主色
    gradient.addColorStop(0, skin.colors.waveformPrimary);
    gradient.addColorStop(1, skin.colors.waveformPrimary);
  }
  
  return gradient;
};
