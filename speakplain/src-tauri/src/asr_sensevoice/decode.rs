pub const VOCAB_SPECIAL_START: usize = 24884; // <|zh|> 起始，跳过语言/情感标记
pub const CTC_BLANK: usize = 0;

/// CTC 贪心解码：去重复 + 去 blank + 过滤特殊标记
pub fn ctc_greedy_decode(tokens: &[String], logits: &[f32], time_steps: usize, vocab_size: usize) -> String {
    let mut prev_id = usize::MAX;
    let mut ids: Vec<usize> = Vec::new();

    for t in 0..time_steps {
        let row = &logits[t * vocab_size..(t + 1) * vocab_size];
        let best = row.iter().enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        if best != CTC_BLANK && best != prev_id {
            ids.push(best);
        }
        prev_id = best;
    }

    decode_tokens(tokens, &ids)
}

fn decode_tokens(tokens: &[String], ids: &[usize]) -> String {
    let mut text = String::new();
    for &id in ids {
        if id >= tokens.len() || id >= VOCAB_SPECIAL_START { continue; }
        let tok = &tokens[id];
        // 跳过 <unk> <s> </s> 等尖括号标记
        if tok.starts_with('<') && tok.ends_with('>') { continue; }
        text.push_str(tok);
    }
    // ▁ (U+2581) 是 SentencePiece 词首空格
    let text = text.replace('\u{2581}', " ");
    let text = text.trim();
    add_cjk_spacing(text)
}

/// 在中文字符和拉丁字符之间插入空格
fn add_cjk_spacing(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 16);
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        result.push(chars[i]);
        if i + 1 < chars.len() {
            let a = is_cjk(chars[i]);
            let b = is_cjk(chars[i + 1]);
            if a != b && chars[i] != ' ' && chars[i + 1] != ' ' {
                result.push(' ');
            }
        }
    }
    result
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF |   // CJK 统一表意文字
        0x3400..=0x4DBF |   // CJK 扩展 A
        0x20000..=0x2A6DF | // CJK 扩展 B
        0x3000..=0x303F |   // CJK 符号和标点
        0xFF00..=0xFFEF     // 全角字符
    )
}
