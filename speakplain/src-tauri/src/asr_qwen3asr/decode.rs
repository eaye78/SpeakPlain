use ort::value::Tensor;
use super::{Qwen3ASREngine, HIDDEN_SIZE, VOCAB_SIZE, NUM_LAYERS, NUM_HEADS, HEAD_DIM, IM_END_ID, ENDOFTEXT_ID};

impl Qwen3ASREngine {
    pub(crate) fn transcribe_chunk(&self, wav: &[f32], language: Option<&str>) -> anyhow::Result<(String, String)> {
        let mel = self.compute_mel_spectrogram(wav);
        let mel_len = mel[0].len();
        let audio_features = self.encode_audio(&mel, mel_len)?;
        let num_audio_tokens = audio_features.len();
        let token_ids = self.build_prompt_ids(num_audio_tokens, language);
        let input_embeds = self.embed_and_fuse(&token_ids, &audio_features);
        let seq_len = token_ids.len();
        let embeds_tensor = Tensor::<f32>::from_array(
            ([1usize, seq_len, HIDDEN_SIZE], input_embeds.clone().into_boxed_slice())
        )?;
        let position_ids: Vec<i64> = (0..seq_len as i64).collect();
        let pos_tensor = Tensor::<i64>::from_array(
            ([1usize, seq_len], position_ids.into_boxed_slice())
        )?;
        let (first_logits, mut present_keys, mut present_values): (Vec<f32>, Vec<f32>, Vec<f32>) = {
            let mut session = self.decoder_init.lock().unwrap();
            let init_out = session.run(ort::inputs! {
                "input_embeds" => embeds_tensor,
                "position_ids" => pos_tensor,
            }).map_err(|e| anyhow::anyhow!("decoder_init 推理失败: {}", e))?;
            let logits_val = init_out.get("logits")
                .ok_or_else(|| anyhow::anyhow!("缺少 logits 输出"))?;
            let (_, logits_data) = logits_val.try_extract_tensor::<f32>()?;
            let keys_val = init_out.get("present_keys")
                .ok_or_else(|| anyhow::anyhow!("缺少 present_keys 输出"))?;
            let (_, keys_data) = keys_val.try_extract_tensor::<f32>()?;
            let values_val = init_out.get("present_values")
                .ok_or_else(|| anyhow::anyhow!("缺少 present_values 输出"))?;
            let (_, values_data) = values_val.try_extract_tensor::<f32>()?;
            (logits_data.to_vec(), keys_data.to_vec(), values_data.to_vec())
        };
        let mut generated: Vec<usize> = Vec::new();
        let mut next_token = self.argmax(&first_logits[(seq_len - 1) * VOCAB_SIZE..seq_len * VOCAB_SIZE]);
        let mut cur_pos = seq_len;
        let max_new_tokens = 512;
        for _ in 0..max_new_tokens {
            if next_token == IM_END_ID || next_token == ENDOFTEXT_ID {
                break;
            }
            generated.push(next_token);
            let mut token_embed = vec![0.0f32; HIDDEN_SIZE];
            for j in 0..HIDDEN_SIZE {
                token_embed[j] = self.embed_tokens[next_token * HIDDEN_SIZE + j];
            }
            let token_tensor = Tensor::<f32>::from_array(
                ([1usize, 1, HIDDEN_SIZE], token_embed.into_boxed_slice())
            )?;
            let pos_tensor = Tensor::<i64>::from_array(
                ([1usize, 1], vec![cur_pos as i64].into_boxed_slice())
            )?;
            let past_k_tensor = Tensor::<f32>::from_array(
                ([NUM_LAYERS, 1usize, NUM_HEADS, cur_pos, HEAD_DIM], present_keys.clone().into_boxed_slice())
            )?;
            let past_v_tensor = Tensor::<f32>::from_array(
                ([NUM_LAYERS, 1usize, NUM_HEADS, cur_pos, HEAD_DIM], present_values.clone().into_boxed_slice())
            )?;
            let step_logits: Vec<f32>;
            {
                let mut session = self.decoder_step.lock().unwrap();
                let step_out = session.run(ort::inputs! {
                    "input_embeds" => token_tensor,
                    "position_ids" => pos_tensor,
                    "past_keys" => past_k_tensor,
                    "past_values" => past_v_tensor,
                }).map_err(|e| anyhow::anyhow!("decoder_step 推理失败: {}", e))?;
                let logits_val = step_out.get("logits")
                    .ok_or_else(|| anyhow::anyhow!("缺少 logits 输出"))?;
                let (_, logits_data) = logits_val.try_extract_tensor::<f32>()?;
                step_logits = logits_data.to_vec();
                let new_keys_val = step_out.get("present_keys")
                    .ok_or_else(|| anyhow::anyhow!("缺少 present_keys 输出"))?;
                let (_, new_keys_data) = new_keys_val.try_extract_tensor::<f32>()?;
                present_keys = new_keys_data.to_vec();
                let new_values_val = step_out.get("present_values")
                    .ok_or_else(|| anyhow::anyhow!("缺少 present_values 输出"))?;
                let (_, new_values_data) = new_values_val.try_extract_tensor::<f32>()?;
                present_values = new_values_data.to_vec();
            }
            next_token = self.argmax(&step_logits[0..VOCAB_SIZE]);
            cur_pos += 1;
        }
        let raw_text = self.decode_ids(&generated);
        let (parsed_lang, parsed_text) = if raw_text.contains("language ") && raw_text.contains("<asr_text>") {
            let parts: Vec<&str> = raw_text.split("<asr_text>").collect();
            let lang_part = parts[0];
            let lang = if lang_part.starts_with("language ") {
                lang_part["language ".len()..].to_string()
            } else {
                String::new()
            };
            let text = if parts.len() > 1 { parts[1].to_string() } else { String::new() };
            (lang, text)
        } else if raw_text.starts_with("language ") {
            let after_language = &raw_text["language ".len()..];
            let known_langs = ["Chinese", "English", "Japanese", "Korean", "French", "German", "Spanish"];
            let mut found_lang = String::new();
            let mut text_start = after_language;
            for lang in &known_langs {
                if after_language.starts_with(lang) {
                    found_lang = lang.to_string();
                    let rest = &after_language[lang.len()..];
                    text_start = rest.trim_start_matches(' ');
                    break;
                }
            }
            (found_lang, text_start.to_string())
        } else if language.is_some() {
            (language.unwrap().to_string(), raw_text)
        } else {
            (String::new(), raw_text)
        };
        Ok((parsed_text, parsed_lang))
    }

    pub(crate) fn decode_ids(&self, ids: &[usize]) -> String {
        let text: String = ids.iter()
            .filter_map(|&id| self.id_to_token.get(&id))
            .cloned()
            .collect::<Vec<_>>()
            .join("");
        let unicode_to_byte = Self::build_unicode_to_byte_map();
        let mut bytes: Vec<u8> = Vec::new();
        for c in text.chars() {
            if let Some(&b) = unicode_to_byte.get(&c) {
                bytes.push(b);
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub(crate) fn build_unicode_to_byte_map() -> std::collections::HashMap<char, u8> {
        let mut bs: Vec<u8> = Vec::new();
        let mut cs: Vec<u32> = Vec::new();
        for b in 0x21u8..=0x7Eu8 { bs.push(b); cs.push(b as u32); }
        for b in 0xA1u8..=0xACu8 { bs.push(b); cs.push(b as u32); }
        for b in 0xAEu8..=0xFFu8 { bs.push(b); cs.push(b as u32); }
        let mut n: u32 = 0;
        for b in 0u8..=255u8 {
            if !bs.contains(&b) {
                bs.push(b);
                cs.push(0x100 + n);
                n += 1;
            }
        }
        let mut map = std::collections::HashMap::new();
        for (b, c) in bs.iter().zip(cs.iter()) {
            map.insert(char::from_u32(*c).unwrap(), *b);
        }
        map
    }

    pub(crate) fn argmax(&self, slice: &[f32]) -> usize {
        slice.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}
