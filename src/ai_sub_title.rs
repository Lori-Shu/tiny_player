use std::{collections::VecDeque, ptr::null_mut};

use ffmpeg_the_third::{
    ChannelLayout,
    ffi::{
        AV_CHANNEL_LAYOUT_MONO, AV_CHANNEL_LAYOUT_STEREO, swr_alloc_set_opts2, swr_convert_frame,
        swr_free, swr_init,
    },
    format::sample::Type,
    frame::Audio,
};

use tracing::{info, warn};
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
    subtitle_source_resampler: ManualProtectedResampler,
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

                                    unsafe {
                                        let mut swr_ctx = null_mut();
                                        let r = swr_alloc_set_opts2(
                                        &mut swr_ctx,
                                        &AV_CHANNEL_LAYOUT_MONO,
                                        ffmpeg_the_third::ffi::AVSampleFormat::AV_SAMPLE_FMT_S16,
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

                                        return Ok(Self {
                                            _chinese_recognize_model: cn_recognize_model,
                                            _english_recognize_model: en_recognize_model,
                                            cn_recognizer: cn_rez,
                                            en_recognizer: en_rez,
                                            source_buffer: VecDeque::new(),
                                            generated_str: String::new(),
                                            subtitle_source_resampler: ManualProtectedResampler(
                                                swr_ctx,
                                            ),
                                        });
                                    }
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
        &mut self,
        audio_frame: ffmpeg_the_third::frame::Audio,
        used_model: UsedModel,
    ) {
        let sub_title = self;
        let mut to_recognize_frame = Audio::empty();
        to_recognize_frame.set_format(ffmpeg_the_third::format::Sample::I16(Type::Packed));
        to_recognize_frame.set_ch_layout(ChannelLayout::MONO);
        to_recognize_frame.set_rate(16000);
        unsafe {
            let resampler = &mut sub_title.subtitle_source_resampler;

            if 0 > swr_convert_frame(
                resampler.0,
                to_recognize_frame.as_mut_ptr(),
                audio_frame.as_ptr(),
            ) {
                warn!("subtitle frame convert err!");
            }
        }
        let data = &bytemuck::cast_slice::<_, i16>(to_recognize_frame.data(0))
            [0..to_recognize_frame.samples()];
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
impl Drop for AISubTitle {
    fn drop(&mut self) {
        unsafe {
            swr_free(&mut self.subtitle_source_resampler.0);
        }
    }
}
