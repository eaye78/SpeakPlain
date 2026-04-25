use ort::value::Tensor;

use super::{Qwen3ASREngine, CHUNK_SIZE, N_MELS, ENCODER_HIDDEN_SIZE, HIDDEN_SIZE, AUDIO_PAD_ID, IM_START_ID, IM_END_ID, NEWLINE_ID, AUDIO_START_ID, AUDIO_END_ID};

impl Qwen3ASREngine {
    pub(crate) fn encode_audio(&self, mel: &[Vec<f32>], mel_len: usize) -> anyhow::Result<Vec<Vec<f32>>> {
        let chunk_num = (mel_len + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let mut chunk_lengths = Vec::new();

        for i in 0..chunk_num {
            let start = i * CHUNK_SIZE;
            let end = (start + CHUNK_SIZE).min(mel_len);
            chunk_lengths.push(end - start);
        }

        let max_chunk_len = *chunk_lengths.iter().max().unwrap();

        let mut padded = vec![0.0f32; chunk_num * 1 * N_MELS * max_chunk_len];
        for i in 0..chunk_num {
            let start = i * CHUNK_SIZE;
            let cl = chunk_lengths[i];
            for m in 0..N_MELS {
                for j in 0..cl {
                    let idx = ((i * N_MELS + m) * max_chunk_len + j) as usize;
                    padded[idx] = mel[m][start + j];
                }
            }
        }

        let padded_tensor = Tensor::<f32>::from_array(
            ([chunk_num, 1, N_MELS, max_chunk_len], padded.into_boxed_slice())
        )?;

        let (conv_shape, conv_data_vec): (Vec<i64>, Vec<f32>) = {
            let mut session = self.encoder_conv.lock().unwrap();
            let conv_out = session.run(ort::inputs! {
                "padded_mel_chunks" => padded_tensor,
            }).map_err(|e| anyhow::anyhow!("encoder_conv 推理失败: {}", e))?;

            let conv_out_val = conv_out.values().next()
                .ok_or_else(|| anyhow::anyhow!("encoder_conv 输出为空"))?;
            let (shape, data) = conv_out_val.try_extract_tensor::<f32>()?;
            (shape.to_vec(), data.to_vec())
        };

        let max_seq_len = conv_shape[1] as usize;
        let conv_hidden_size = ENCODER_HIDDEN_SIZE;
        let mut features: Vec<f32> = Vec::new();

        for i in 0..chunk_num {
            let lens_after_cnn = Self::get_feat_extract_output_lengths(chunk_lengths[i]);
            let chunk_offset = i * max_seq_len;
            for j in 0..lens_after_cnn.min(max_seq_len) {
                let idx = chunk_offset + j;
                let data_offset = idx * conv_hidden_size;
                if data_offset + conv_hidden_size <= conv_data_vec.len() {
                    for k in 0..conv_hidden_size {
                        features.push(conv_data_vec[data_offset + k]);
                    }
                }
            }
        }

        let total_tokens = features.len() / conv_hidden_size;

        let hidden_states = Tensor::<f32>::from_array(
            ([total_tokens, conv_hidden_size], features.into_boxed_slice())
        )?;

        let attn_mask = Tensor::<f32>::from_array(
            ([1usize, 1, total_tokens, total_tokens], vec![0.0f32; total_tokens * total_tokens].into_boxed_slice())
        )?;

        let out_data_vec: Vec<f32> = {
            let mut session = self.encoder_transformer.lock().unwrap();
            let encoder_out = session.run(ort::inputs! {
                "hidden_states" => hidden_states,
                "attention_mask" => attn_mask,
            }).map_err(|e| anyhow::anyhow!("encoder_transformer 推理失败: {}", e))?;

            let out_val = encoder_out.values().next()
                .ok_or_else(|| anyhow::anyhow!("encoder_transformer 输出为空"))?;
            let (_, data) = out_val.try_extract_tensor::<f32>()?;
            data.to_vec()
        };

        let mut result = Vec::new();
        for i in 0..total_tokens {
            let mut row = Vec::new();
            for j in 0..HIDDEN_SIZE {
                row.push(out_data_vec[i * HIDDEN_SIZE + j]);
            }
            result.push(row);
        }

        Ok(result)
    }

    pub(crate) fn build_prompt_ids(&self, num_audio_tokens: usize, language: Option<&str>) -> Vec<usize> {
        let mut ids = Vec::new();

        ids.push(IM_START_ID);
        ids.extend(self.encode_text("system"));
        ids.push(NEWLINE_ID);
        ids.push(IM_END_ID);
        ids.push(NEWLINE_ID);

        ids.push(IM_START_ID);
        ids.extend(self.encode_text("user"));
        ids.push(NEWLINE_ID);
        ids.push(AUDIO_START_ID);
        ids.extend(vec![AUDIO_PAD_ID; num_audio_tokens]);
        ids.push(AUDIO_END_ID);
        ids.push(IM_END_ID);
        ids.push(NEWLINE_ID);

        ids.push(IM_START_ID);
        ids.extend(self.encode_text("assistant"));
        ids.push(NEWLINE_ID);

        if let Some(lang) = language {
            let lang_text = format!("language {}<asr_text>", lang);
            ids.extend(self.encode_text(&lang_text));
        }

        ids
    }

    pub(crate) fn encode_text(&self, text: &str) -> Vec<usize> {
        let mut result = Vec::new();
        let mut remaining = text;
        while !remaining.is_empty() {
            let mut longest_id: Option<usize> = None;
            let mut longest_len = 0usize;
            for (token, &id) in &self.vocab {
                if remaining.starts_with(token.as_str()) && token.len() > longest_len {
                    longest_id = Some(id);
                    longest_len = token.len();
                }
            }
            if let Some(id) = longest_id {
                result.push(id);
                remaining = &remaining[longest_len..];
            } else {
                let c = remaining.chars().next().unwrap();
                remaining = &remaining[c.len_utf8()..];
            }
        }
        result
    }

    pub(crate) fn embed_and_fuse(&self, token_ids: &[usize], audio_features: &[Vec<f32>]) -> Vec<f32> {
        let seq_len = token_ids.len();
        let mut embeds = vec![0.0f32; seq_len * HIDDEN_SIZE];

        for (i, &id) in token_ids.iter().enumerate() {
            for j in 0..HIDDEN_SIZE {
                embeds[i * HIDDEN_SIZE + j] = self.embed_tokens[id * HIDDEN_SIZE + j];
            }
        }

        let mut audio_pos = 0;
        for (i, &id) in token_ids.iter().enumerate() {
            if id == AUDIO_PAD_ID && audio_pos < audio_features.len() {
                for j in 0..HIDDEN_SIZE {
                    embeds[i * HIDDEN_SIZE + j] = audio_features[audio_pos][j];
                }
                audio_pos += 1;
            }
        }

        embeds
    }
}
