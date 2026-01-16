use std::{fs::File, ptr::null_mut, sync::Arc};

use candle_core::IndexOp;
use candle_nn::ops::softmax;
use candle_transformers::{
    models::{
        mimi::candle::{Device, Tensor},
        whisper::{self, Config, quantized_model::Whisper},
    },
    quantized_var_builder::VarBuilder,
};
use ffmpeg_the_third::{
    ChannelLayout,
    ffi::{
        AV_CHANNEL_LAYOUT_MONO, AV_CHANNEL_LAYOUT_STEREO, swr_alloc_set_opts2, swr_convert_frame,
        swr_free, swr_init,
    },
    format::sample::Type,
    frame::Audio,
};

use tokenizers::{Tokenizer, tokenizer};
use tokio::sync::{RwLock, mpsc::Sender};
use tracing::{info, warn};

use crate::{CURRENT_EXE_PATH, PlayerError, PlayerResult, decode::ManualProtectedResampler};
const MEL_FILTERS: &[u8] = include_bytes!("../resources/model/melfilters.bytes");
#[derive(PartialEq, Clone)]
pub enum UsedModel {
    Empty,
    Chinese,
    English,
}

pub struct AISubTitle {
    mel_filters: Vec<f32>,
    config: Config,
    device: Device,
    recognizer: Whisper,
    tokenizer: Tokenizer,
    sot_id: u32,
    zh_language_id: u32,
    en_language_id: u32,
    transcribe_id: u32,
    no_timestamp_id: u32,
    eot_id: u32,
    no_speech_id: u32,
    source_buffer: Vec<f32>,
    subtitle_source_resampler: ManualProtectedResampler,
    subtitle_sender: Sender<String>,
}
impl AISubTitle {
    pub fn new(subtitle_sender: Sender<String>) -> PlayerResult<Self> {
        let current_exe_path = CURRENT_EXE_PATH.as_ref().map_err(|e| e.clone())?;
        if let Some(folder_path) = current_exe_path.parent() {
            let model_path = folder_path.join("model/base_q8_0.gguf");
            let config_path = folder_path.join("model/config.json");
            let tokenizer_path = folder_path.join("model/tokenizer.json");
            if let Some(model_path_str) = model_path.to_str() {
                // if let Err(e)=Device::new_cuda(0){
                //     warn!("cuda init error! {}",e.to_string());
                // }
                let device =
                    Device::new_cuda(0).map_err(|e| PlayerError::Internal(e.to_string()))?;
                let var_builder = VarBuilder::from_gguf(model_path_str, &device)
                    .map_err(|e| PlayerError::Internal(e.to_string()))?;

                let config = serde_json::from_reader::<_, Config>(
                    File::open(config_path).map_err(|e| PlayerError::Internal(e.to_string()))?,
                )
                .map_err(|e| PlayerError::Internal(e.to_string()))?;
                let whisper_recognizer =
                    candle_transformers::models::whisper::quantized_model::Whisper::load(
                        &var_builder,
                        config.clone(),
                    )
                    .map_err(|e| PlayerError::Internal(e.to_string()))?;
                let tokenizer = tokenizer::Tokenizer::from_file(tokenizer_path)
                    .map_err(|e| PlayerError::Internal(e.to_string()))?;

                unsafe {
                    let mut swr_ctx = null_mut();
                    let r = swr_alloc_set_opts2(
                        &mut swr_ctx,
                        &AV_CHANNEL_LAYOUT_MONO,
                        ffmpeg_the_third::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT,
                        16000,
                        &AV_CHANNEL_LAYOUT_STEREO,
                        ffmpeg_the_third::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT,
                        48000,
                        0,
                        null_mut(),
                    );
                    if r < 0 {
                        info!("swr ctx create err");
                    }
                    let r = swr_init(swr_ctx);
                    if r < 0 {
                        info!("swr init err");
                    }
                    let mut mel_filters = vec![0f32; MEL_FILTERS.len() / 4];
                    <byteorder::LittleEndian as byteorder::ByteOrder>::read_f32_into(
                        MEL_FILTERS,
                        &mut mel_filters,
                    );
                    let sot_id = tokenizer
                        .token_to_id(candle_transformers::models::whisper::SOT_TOKEN)
                        .ok_or(PlayerError::Internal("sot token to id err".to_string()))?;

                    let transcribe_id = tokenizer
                        .token_to_id(candle_transformers::models::whisper::TRANSCRIBE_TOKEN)
                        .ok_or(PlayerError::Internal(
                            "TRANSCRIBE_TOKEN to id err".to_string(),
                        ))?;

                    let no_timestamp_id = tokenizer
                        .token_to_id(candle_transformers::models::whisper::NO_TIMESTAMPS_TOKEN)
                        .ok_or(PlayerError::Internal(
                            "NO_TIMESTAMPS_TOKEN to id err".to_string(),
                        ))?;
                    let eot_id = tokenizer
                        .token_to_id(candle_transformers::models::whisper::EOT_TOKEN)
                        .ok_or(PlayerError::Internal("EOT_TOKEN to id err".to_string()))?;
                    let no_speech_token = whisper::NO_SPEECH_TOKENS
                        .iter()
                        .find_map(|token| tokenizer.token_to_id(token));
                    let no_speech_id = match no_speech_token {
                        None => {
                            return Err(PlayerError::Internal(
                                "unable to find any non-speech token".to_string(),
                            ));
                        }
                        Some(n) => n,
                    };
                    return Ok(Self {
                        subtitle_sender,
                        sot_id,
                        zh_language_id: 50260,
                        en_language_id: 50259,
                        transcribe_id,
                        no_timestamp_id,
                        eot_id,
                        no_speech_id,
                        mel_filters,
                        config,
                        device,
                        tokenizer,
                        recognizer: whisper_recognizer,
                        source_buffer: Vec::new(),
                        subtitle_source_resampler: ManualProtectedResampler(swr_ctx),
                    });
                }
            }
        }

        Err(PlayerError::Internal(
            "AISubTitle construct err".to_string(),
        ))
    }

    pub fn push_frame_data(
        subtitle: Arc<RwLock<AISubTitle>>,
        audio_frame: ffmpeg_the_third::frame::Audio,
        used_model: UsedModel,
    ) {
        let mut subtitle = subtitle.blocking_write();
        let mut to_recognize_frame = Audio::empty();
        to_recognize_frame.set_format(ffmpeg_the_third::format::Sample::F32(Type::Packed));
        to_recognize_frame.set_ch_layout(ChannelLayout::MONO);
        to_recognize_frame.set_rate(16000);
        unsafe {
            let resampler = &mut subtitle.subtitle_source_resampler;

            if 0 > swr_convert_frame(
                resampler.0,
                to_recognize_frame.as_mut_ptr(),
                audio_frame.as_ptr(),
            ) {
                warn!("subtitle frame convert err!");
            }
        }
        let data = &bytemuck::cast_slice::<_, f32>(to_recognize_frame.data(0))
            [0..to_recognize_frame.samples()];
        subtitle.source_buffer.extend(data);
        if subtitle.source_buffer.len() > 16000 * 2 {
            let mel_source = candle_transformers::models::whisper::audio::pcm_to_mel(
                &subtitle.config,
                &subtitle.source_buffer,
                &subtitle.mel_filters,
            );

            let mel_len = mel_source.len();
            // 转换为 Tensor 并增加 Batch 维度 [1, 80, 3000]
            if let Ok(tensor) = Tensor::from_vec(
                mel_source,
                (
                    1,
                    subtitle.config.num_mel_bins,
                    mel_len / subtitle.config.num_mel_bins,
                ),
                &subtitle.device,
            ) {
                warn!("before forward");
                if let Err(e) = subtitle.recognizer.encoder.forward(&tensor, true) {
                    warn!("forward error {}", e.to_string());
                }
                if let Ok(audio_features) = subtitle.recognizer.encoder.forward(&tensor, true) {
                    if let UsedModel::Chinese = &used_model {
                        let mut tokens = vec![
                            subtitle.sot_id,
                            subtitle.zh_language_id,
                            subtitle.transcribe_id,
                            subtitle.no_timestamp_id,
                        ];
                        let max_tokens = subtitle.config.max_target_positions / 8;
                        warn!("before loop");
                        // 2. 开始循环
                        for i in 0..max_tokens {
                            if let Ok(input_ids) = Tensor::new(tokens.as_slice(), &subtitle.device)
                            {
                                warn!("before unsqueeze");
                                if let Ok(input_ids) = input_ids.unsqueeze(0) {
                                    warn!("before decode");
                                    if let Ok(ys) = subtitle.recognizer.decoder.forward(
                                        &input_ids,
                                        &audio_features,
                                        true,
                                    ) {
                                        if i == 0 {
                                            if let Ok(t) = ys.i(..1) {
                                                if let Ok(t) =
                                                    subtitle.recognizer.decoder.final_linear(&t)
                                                {
                                                    if let Ok(t) = t.i(0) {
                                                        if let Ok(logits) = t.i(0) {
                                                            if let Ok(n) = softmax(&logits, 0) {
                                                                if let Ok(n) = n
                                                                    .i(subtitle.no_speech_id
                                                                        as usize)
                                                                {
                                                                    if let Ok(no_speech_prob) =
                                                                        n.to_scalar::<f32>()
                                                                    {
                                                                        if (no_speech_prob as f64)>candle_transformers::models::whisper::NO_SPEECH_THRESHOLD {
                                                                                    break;
                                                                                }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // 获取序列长度并取最后一个 Token 的 Logits
                                        warn!("before dims3");
                                        if let Ok((_, seq_len, _)) = ys.dims3() {
                                            if let Ok(t) = ys.i((..1, seq_len - 1..)) {
                                                if let Ok(t) =
                                                    subtitle.recognizer.decoder.final_linear(&t)
                                                {
                                                    if let Ok(t) = t.i(0) {
                                                        if let Ok(logits) = t.i(0) {
                                                            let logits_v: candle_core::Result<
                                                                Vec<f32>,
                                                            > = logits.to_vec1();
                                                            if let Ok(logits_v) = logits_v {
                                                                if let Some(v) = logits_v
                                                                    .iter()
                                                                    .enumerate()
                                                                    .max_by(|(_, u), (_, v)| {
                                                                        u.total_cmp(v)
                                                                    })
                                                                    .map(|(i, _)| i as u32)
                                                                {
                                                                    let next_token = v;
                                                                    tokens.push(next_token);
                                                                    if next_token == subtitle.eot_id
                                                                        || tokens.len() > subtitle.config.max_target_positions/8
                                                                    {
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Ok(text) = subtitle.tokenizer.decode(&tokens, true) {
                            subtitle.source_buffer.drain(0..(16000 * 2));
                            if let Err(e) = subtitle
                                .subtitle_sender
                                .blocking_send(text.trim().to_string())
                            {
                                warn!("send subtitle err {}", e.to_string());
                            }
                        }
                    } else if let UsedModel::English = &used_model {
                        let mut tokens = vec![
                            subtitle.sot_id,
                            subtitle.en_language_id,
                            subtitle.transcribe_id,
                            subtitle.no_timestamp_id,
                        ];
                        let max_tokens = subtitle.config.max_target_positions / 8;
                        warn!("before loop");
                        // 2. 开始循环
                        for i in 0..max_tokens {
                            if let Ok(input_ids) = Tensor::new(tokens.as_slice(), &subtitle.device)
                            {
                                warn!("before unsqueeze");
                                if let Ok(input_ids) = input_ids.unsqueeze(0) {
                                    warn!("before decode");
                                    if let Ok(ys) = subtitle.recognizer.decoder.forward(
                                        &input_ids,
                                        &audio_features,
                                        true,
                                    ) {
                                        if i == 0 {
                                            if let Ok(t) = ys.i(..1) {
                                                if let Ok(t) =
                                                    subtitle.recognizer.decoder.final_linear(&t)
                                                {
                                                    if let Ok(t) = t.i(0) {
                                                        if let Ok(logits) = t.i(0) {
                                                            if let Ok(n) = softmax(&logits, 0) {
                                                                if let Ok(n) = n
                                                                    .i(subtitle.no_speech_id
                                                                        as usize)
                                                                {
                                                                    if let Ok(no_speech_prob) =
                                                                        n.to_scalar::<f32>()
                                                                    {
                                                                        if (no_speech_prob as f64)>candle_transformers::models::whisper::NO_SPEECH_THRESHOLD {
                                                                            break;
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // 获取序列长度并取最后一个 Token 的 Logits
                                        warn!("before dims3");
                                        if let Ok((_, seq_len, _)) = ys.dims3() {
                                            // 3. ✨ 关键：切出最后一个位置的隐藏状态 [1, 1, hidden_dim]
                                            // 这样做是为了效率，因为我们只需要预测下一个 token
                                            if let Ok(t) = ys.i((..1, seq_len - 1..)) {
                                                if let Ok(t) =
                                                    subtitle.recognizer.decoder.final_linear(&t)
                                                {
                                                    if let Ok(t) = t.i(0) {
                                                        if let Ok(logits) = t.i(0) {
                                                            let logits_v: candle_core::Result<
                                                                Vec<f32>,
                                                            > = logits.to_vec1();
                                                            if let Ok(logits_v) = logits_v {
                                                                if let Some(v) = logits_v
                                                                    .iter()
                                                                    .enumerate()
                                                                    .max_by(|(_, u), (_, v)| {
                                                                        u.total_cmp(v)
                                                                    })
                                                                    .map(|(i, _)| i as u32)
                                                                {
                                                                    let next_token = v;
                                                                    tokens.push(next_token);
                                                                    if next_token == subtitle.eot_id
                                                                        || tokens.len() > subtitle.config.max_target_positions/8
                                                                    {
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Ok(text) = subtitle.tokenizer.decode(&tokens, true) {
                            subtitle.source_buffer.drain(0..(16000 * 2));
                            if let Err(e) = subtitle
                                .subtitle_sender
                                .blocking_send(text.trim().to_string())
                            {
                                warn!("send subtitle err {}", e.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Drop for AISubTitle {
    fn drop(&mut self) {
        unsafe {
            swr_free(&mut self.subtitle_source_resampler.0);
        }
    }
}
