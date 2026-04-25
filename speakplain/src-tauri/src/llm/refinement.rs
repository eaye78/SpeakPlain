// LLM 润色流程与内置人设

use log::info;

use crate::llm::types::{LlmProviderConfig, Persona};
use crate::llm::providers::create_provider;

// ─── 内置人设 ─────────────────────────────────────────────────────────────────

pub fn builtin_personas() -> Vec<Persona> {
    vec![
        Persona {
            id: "formal".into(),
            name: "正式书面".into(),
            description: Some("工作邮件、汇报、公文".into()),
            system_prompt: "你是一个文字润色助手。将用户输入的口语化文字改写为正式、书面的表达方式。\n要求：\n- 保持原意，不添加、不删减实质内容\n- 使用正式语气，避免口语化词汇\n- 语句通顺，标点规范\n- 直接输出改写后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "concise".into(),
            name: "简洁精炼".into(),
            description: Some("备忘录、清单、摘要".into()),
            system_prompt: "你是一个文字压缩助手。将用户输入的文字提炼为简洁、清晰的表达。\n要求：\n- 去除冗余词语和重复内容\n- 保留核心信息\n- 每个要点尽量控制在一句话内\n- 直接输出改写后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "casual".into(),
            name: "口语自然".into(),
            description: Some("聊天、日常沟通".into()),
            system_prompt: "你是一个文字整理助手。将用户输入的语音文字整理为自然、流畅的口语表达。\n要求：\n- 保持口语风格，但去除明显的停顿词（嗯、啊、那个等）\n- 语句连贯，易于阅读\n- 保持原有的情感和语气\n- 直接输出整理后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "logical".into(),
            name: "逻辑严谨".into(),
            description: Some("技术文档、分析报告".into()),
            system_prompt: "你是一个逻辑整理助手。将用户输入的文字重新组织为条理清晰、逻辑严谨的表达。\n要求：\n- 识别并明确论点、论据、结论\n- 使用条件、因果、递进等逻辑连接词\n- 如有多个要点，使用编号或分段表达\n- 直接输出整理后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "creative".into(),
            name: "创意文案".into(),
            description: Some("营销、推广、故事".into()),
            system_prompt: "你是一个创意写作助手。将用户输入的文字改写为生动、有感染力的创意表达。\n要求：\n- 可以适当使用比喻、排比等修辞手法\n- 语言生动，有画面感\n- 保持原意的核心信息\n- 直接输出创作后的文字，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "translator".into(),
            name: "中译英".into(),
            description: Some("输入中文，输出英文翻译".into()),
            system_prompt: "你是一个专业翻译。将用户输入的中文翻译为自然流畅的英文。\n要求：\n- 准确传达原文含义\n- 使用地道的英文表达，避免直译\n- 保持原文的语气和风格\n- 直接输出翻译结果，不加任何解释或前缀".into(),
            is_builtin: true,
        },
        Persona {
            id: "en_to_zh".into(),
            name: "英译中".into(),
            description: Some("输入英文，输出中文翻译".into()),
            system_prompt: "你是一个专业翻译。将用户输入的英文翻译为自然流畅的中文。\n要求：\n- 准确传达原文含义\n- 使用地道的中文表达\n- 保持原文的语气和风格\n- 直接输出翻译结果，不加任何解释或前缀".into(),
            is_builtin: true,
        },
    ]
}

// ─── 核心润色入口（供 main.rs 调用） ─────────────────────────────────────────

/// 移除 LLM 输出中的 <think>...</think> 思考块（支持嵌套、跨行）
fn regex_strip_think(s: &str) -> String {
    let mut result = String::new();
    let mut rest = s;
    loop {
        if let Some(start) = rest.find("<think>") {
            result.push_str(&rest[..start]);
            let after = &rest[start + 7..];
            if let Some(end) = after.find("</think>") {
                rest = &after[end + 8..];
            } else {
                result.push_str(after);
                break;
            }
        } else {
            result.push_str(rest);
            break;
        }
    }
    result.trim().to_string()
}

/// 根据当前配置对 `raw_text` 进行 LLM 润色。
/// 失败时返回 Err，调用方应降级使用原始文字。
/// 完整润色流程：构建 provider → 调用 refine → 返回润色文字
pub async fn do_refine(
    provider_config: &LlmProviderConfig,
    persona: &Persona,
    raw_text: &str,
) -> anyhow::Result<String> {
    let provider = create_provider(provider_config.clone())?;
    let result = provider.refine(&persona.system_prompt, raw_text).await?;
    // 过滤掉 <think>...</think> 思考过程，只保留实际结果
    let result = regex_strip_think(&result);
    info!("[LLM] 润色完成: {} chars → {} chars", raw_text.len(), result.len());
    Ok(result)
}

/// 连接测试
pub async fn test_provider(provider_config: LlmProviderConfig) -> anyhow::Result<String> {
    let provider = create_provider(provider_config)?;
    provider.ping().await
}
