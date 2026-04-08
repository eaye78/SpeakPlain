/**
 * 皮肤系统类型定义
 * 支持文件夹形式的皮肤，每个皮肤包含：
 * - skin.json: 主配置文件
 * - styles.css: 样式文件
 * - background.png: 背景图片（可选）
 */

// 颜色配置
export interface SkinColors {
  background: string;
  backgroundGradient?: string;
  textPrimary: string;
  textSecondary: string;
  textActive: string;
  waveformPrimary: string;
  waveformSecondary?: string;
  waveformGradient?: string;
  dragDot: string;
  dragDotHover?: string;
  processingDot: string;
  shadowLight: string;
  shadowDark: string;
}

// 尺寸配置
export interface SkinDimensions {
  borderRadius: number;
  paddingX: number;
  paddingY: number;
  gap: number;
}

// 动画配置
export interface SkinAnimations {
  transitionDuration: string;
}

// 完整皮肤配置（从 skin.json 读取）
export interface SkinConfig {
  id: string;
  name: string;
  description?: string;
  author?: string;
  version?: string;
  isBuiltIn: boolean;
  /** 是否有背景图片 */
  hasBackgroundImage: boolean;
  /** 皮肤文件夹路径（用于加载样式和图片） */
  folderPath?: string;
  colors: SkinColors;
  dimensions: SkinDimensions;
  animations: SkinAnimations;
}

// 皮肤列表项（用于选择器显示）
export interface SkinListItem {
  id: string;
  name: string;
  description?: string;
  isBuiltIn: boolean;
  isCustom: boolean;
  previewColor: string;
  /** 是否有背景图片 */
  hasBackgroundImage: boolean;
}

// skin.json 文件结构
export interface SkinJson {
  id: string;
  name: string;
  description?: string;
  author?: string;
  version?: string;
  hasBackgroundImage?: boolean;
  colors: {
    background?: string;
    backgroundGradient?: string;
    textPrimary?: string;
    textSecondary?: string;
    textActive?: string;
    waveformPrimary?: string;
    waveformSecondary?: string;
    waveformGradient?: string;
    dragDot?: string;
    dragDotHover?: string;
    processingDot?: string;
    shadowLight?: string;
    shadowDark?: string;
  };
  dimensions?: {
    borderRadius?: number;
    paddingX?: number;
    paddingY?: number;
    gap?: number;
  };
  animations?: {
    transitionDuration?: string;
  };
}

// 自定义皮肤JSON文件结构（向后兼容）
export interface CustomSkinJson extends SkinJson {}

// 加载的皮肤资源
export interface SkinResources {
  config: SkinConfig;
  cssText?: string;
  backgroundUrl?: string;
}
