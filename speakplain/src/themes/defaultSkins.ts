/**
 * 默认内置皮肤配置
 * 彩虹、沙漠、星空三套主题
 */
import type { SkinConfig } from "./skinTypes";

// 彩虹主题 - 活力渐变
export const rainbowSkin: SkinConfig = {
  id: "rainbow",
  name: "彩虹",
  description: "活力渐变彩虹色，充满青春气息",
  isBuiltIn: true,
  hasBackgroundImage: false,
  folderPath: "skins/rainbow",
  colors: {
    background: "#e8ecf3",
    backgroundGradient: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
    textPrimary: "#4a5568",
    textSecondary: "#8a9ab8",
    textActive: "#667eea",
    waveformPrimary: "#667eea",
    waveformSecondary: "#764ba2",
    waveformGradient: "linear-gradient(90deg, #667eea 0%, #764ba2 100%)",
    dragDot: "#b0bcd0",
    dragDotHover: "#667eea",
    processingDot: "#667eea",
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

// 沙漠主题 - 温暖大地
export const desertSkin: SkinConfig = {
  id: "desert",
  name: "沙漠",
  description: "温暖的大地色调，沉稳舒适",
  isBuiltIn: true,
  hasBackgroundImage: false,
  folderPath: "skins/desert",
  colors: {
    background: "#f5e6d3",
    backgroundGradient: "linear-gradient(135deg, #d4a574 0%, #c17f4e 100%)",
    textPrimary: "#5d4e37",
    textSecondary: "#a08060",
    textActive: "#c17f4e",
    waveformPrimary: "#c17f4e",
    waveformSecondary: "#d4a574",
    waveformGradient: "linear-gradient(90deg, #d4a574 0%, #c17f4e 100%)",
    dragDot: "#c4a882",
    dragDotHover: "#c17f4e",
    processingDot: "#c17f4e",
    shadowLight: "#fff8f0",
    shadowDark: "#d4c4b0",
  },
  dimensions: {
    borderRadius: 10,
    paddingX: 14,
    paddingY: 0,
    gap: 8,
  },
  animations: {
    transitionDuration: "0.3s",
  },
};

// 星空主题 - 深邃神秘
export const starrySkin: SkinConfig = {
  id: "starry",
  name: "星空",
  description: "深邃星空配色，科技感十足",
  isBuiltIn: true,
  hasBackgroundImage: false,
  folderPath: "skins/starry",
  colors: {
    background: "#1a1f2e",
    backgroundGradient: "linear-gradient(135deg, #1a1f2e 0%, #2d3748 50%, #1a202c 100%)",
    textPrimary: "#e2e8f0",
    textSecondary: "#718096",
    textActive: "#63b3ed",
    waveformPrimary: "#63b3ed",
    waveformSecondary: "#4299e1",
    waveformGradient: "linear-gradient(90deg, #4299e1 0%, #63b3ed 50%, #90cdf4 100%)",
    dragDot: "#4a5568",
    dragDotHover: "#63b3ed",
    processingDot: "#63b3ed",
    shadowLight: "#2d3748",
    shadowDark: "#0d1117",
  },
  dimensions: {
    borderRadius: 14,
    paddingX: 14,
    paddingY: 0,
    gap: 8,
  },
  animations: {
    transitionDuration: "0.3s",
  },
};

// 经典主题 - 简洁默认
export const classicSkin: SkinConfig = {
  id: "classic",
  name: "经典",
  description: "简洁经典，还原最初的设计",
  isBuiltIn: true,
  hasBackgroundImage: false,
  folderPath: "skins/classic",
  colors: {
    background: "#e8ecf3",
    textPrimary: "#4a5568",
    textSecondary: "#8a9ab8",
    textActive: "#2563eb",
    waveformPrimary: "#3b6beb",
    dragDot: "#b0bcd0",
    dragDotHover: "#2563eb",
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

// 所有内置皮肤
export const builtInSkins: SkinConfig[] = [
  classicSkin,
  rainbowSkin,
  desertSkin,
  starrySkin,
];

// 获取默认皮肤
export const getDefaultSkin = (): SkinConfig => classicSkin;

// 根据ID获取内置皮肤
export const getBuiltInSkinById = (id: string): SkinConfig | undefined => {
  return builtInSkins.find((skin) => skin.id === id);
};
