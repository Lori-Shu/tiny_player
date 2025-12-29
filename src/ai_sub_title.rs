use std::{collections::VecDeque, sync::Arc};

use ffmpeg_the_third::{ChannelLayout, format::Sample, frame::Audio};
use log::warn;
use tokio::sync::RwLock;
use vosk::{DecodingState, Model, Recognizer};

use crate::{CURRENT_EXE_PATH, PlayerError, PlayerResult, decode::ManualProtectedResampler};
#[derive(PartialEq, Clone)]
pub enum UsedModel {
    Empty,
    Chinese,
    English,
}
pub struct AISubTitle {
    _chinese_recognize_model: Model,
    _english_recognize_model: Model,
    cn_recognizer: Recognizer,
    en_recognizer: Recognizer,
    source_buffer: VecDeque<i16>,
    generated_str: String,
    subtitle_source_resampler: Arc<RwLock<ManualProtectedResampler>>,
}
impl AISubTitle {
    pub fn new() -> PlayerResult<Self> {
        let current_exe_path = CURRENT_EXE_PATH.as_ref().map_err(|e| e.clone())?;
        if let Some(folder_path) = current_exe_path.parent() {
            let chinese_model_path = folder_path.join("model/vosk-model-small-cn-0.22");
            let english_model_path = folder_path.join("model/vosk-model-small-en-us-0.15");
            if let Some(cn_model_path_str) = chinese_model_path.to_str() {
                if let Some(en_model_path_str) = english_model_path.to_str() {
                    if let Some(cn_recognize_model) = Model::new(cn_model_path_str) {
                        if let Some(en_recognize_model) = Model::new(en_model_path_str) {
                            if let Some(mut cn_rez) = Recognizer::new(&cn_recognize_model, 16000.0)
                            {
                                if let Some(mut en_rez) =
                                    Recognizer::new(&en_recognize_model, 16000.0)
                                {
                                    cn_rez.set_max_alternatives(0);
                                    cn_rez.set_partial_words(true);
                                    cn_rez.set_nlsml(true);
                                    en_rez.set_max_alternatives(0);
                                    en_rez.set_partial_words(true);
                                    en_rez.set_nlsml(true);
                                    let resampler = ffmpeg_the_third::software::resampler2(
                            (
                                Sample::F32(ffmpeg_the_third::util::format::sample::Type::Packed),
                                ChannelLayout::STEREO,
                                48000,
                            ),
                            (
                                Sample::I16(ffmpeg_the_third::util::format::sample::Type::Packed),
                                ChannelLayout::MONO,
                                16000,
                            ),
                        ).map_err(|e|PlayerError::Internal(e.to_string()))?;
                                    return Ok(Self {
                                        _chinese_recognize_model: cn_recognize_model,
                                        _english_recognize_model: en_recognize_model,
                                        cn_recognizer: cn_rez,
                                        en_recognizer: en_rez,
                                        source_buffer: VecDeque::new(),
                                        generated_str: String::new(),
                                        subtitle_source_resampler: Arc::new(RwLock::new(
                                            ManualProtectedResampler(resampler),
                                        )),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(PlayerError::Internal(
            "AISubTitle construct err".to_string(),
        ))
    }

    pub async fn push_frame_data(
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        audio_frame: ffmpeg_the_third::frame::Audio,
        used_model: UsedModel,
    ) {
        let mut sub_title = ai_subtitle.write().await;
        let mut to_recognize_frame = Audio::empty();
        {
            let mut resampler = sub_title.subtitle_source_resampler.write().await;
            if resampler
                .0
                .run(&audio_frame, &mut to_recognize_frame)
                .is_err()
            {
                warn!("subtitle frame convert err!");
            }
        }
        let data = unsafe {
            std::slice::from_raw_parts::<'static, i16>(
                to_recognize_frame.data(0).as_ptr() as *const i16,
                to_recognize_frame.samples(),
            )
        };
        sub_title.source_buffer.extend(data);
        if let UsedModel::Chinese = &used_model {
            if sub_title.source_buffer.len() > 3200 {
                let buf = sub_title.source_buffer.drain(0..3200).collect::<Vec<i16>>();
                if let Ok(state) = sub_title.cn_recognizer.accept_waveform(&buf) {
                    let partial_result = sub_title.cn_recognizer.partial_result();
                    if !partial_result.partial.is_empty() {
                        let no_space_result = partial_result.partial.replace(" ", "");
                        sub_title.generated_str = no_space_result;
                    }
                    if let DecodingState::Finalized = state {
                        if let Some(res) = sub_title.cn_recognizer.final_result().single() {
                            let no_space_result = res.text.replace(" ", "");
                            sub_title.generated_str = no_space_result;
                        }
                    }
                }
            }
        } else if let UsedModel::English = &used_model {
            if sub_title.source_buffer.len() > 3200 {
                let buf = sub_title.source_buffer.drain(0..3200).collect::<Vec<i16>>();
                if let Ok(state) = sub_title.en_recognizer.accept_waveform(&buf) {
                    let partial_result = sub_title.en_recognizer.partial_result();
                    if !partial_result.partial.is_empty() {
                        sub_title.generated_str = partial_result.partial.to_string();
                    }
                    if let DecodingState::Finalized = state {
                        if let Some(res) = sub_title.en_recognizer.final_result().single() {
                            sub_title.generated_str = res.text.to_string();
                        }
                    }
                }
            }
        }
    }
    pub fn generated_str(&self) -> &str {
        &self.generated_str
    }
}
