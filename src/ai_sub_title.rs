use std::{collections::VecDeque, sync::Arc};

use ffmpeg_the_third::{ChannelLayout, format::Sample, frame::Audio};
use tokio::sync::RwLock;
use vosk::{DecodingState, Model, Recognizer};

use crate::{CURRENT_EXE_PATH, PlayerError, PlayerResult, decode::ManualProtectedResampler};

pub struct AISubTitle {
    _chinese_recognize_model: Model,
    recognizer: Recognizer,
    source_buffer: VecDeque<i16>,
    generated_str: String,
    subtitle_source_resampler: Arc<RwLock<ManualProtectedResampler>>,
}
impl AISubTitle {
    pub fn new() -> PlayerResult<Self> {
        let current_exe_path = CURRENT_EXE_PATH.as_ref().map_err(|e| e.clone())?;
        if let Some(folder_path) = current_exe_path.parent() {
            let model_path = folder_path.join("model/vosk-model-small-cn-0.22");
            if let Some(model_path_str) = model_path.to_str() {
                if let Some(recognize_model) = Model::new(model_path_str) {
                    if let Some(mut rez) = Recognizer::new(&recognize_model, 16000.0) {
                        rez.set_max_alternatives(0);
                        rez.set_partial_words(true);
                        rez.set_nlsml(true);
                        if let Ok(resampler) = ffmpeg_the_third::software::resampler2(
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
                        ) {
                            return Ok(Self {
                                _chinese_recognize_model: recognize_model,
                                recognizer: rez,
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
        Err(PlayerError::Internal(
            "AISubTitle construct err".to_string(),
        ))
    }

    pub async fn push_frame_data(
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        audio_frame: ffmpeg_the_third::frame::Audio,
    ) {
        let mut sub_title = ai_subtitle.write().await;
        let mut to_recognize_frame = Audio::empty();
        {
            let mut resampler = sub_title.subtitle_source_resampler.write().await;
            if resampler
                .0
                .run(&audio_frame, &mut to_recognize_frame)
                .is_ok()
            {}
        }
        let data: &[i16] = unsafe {
            std::slice::from_raw_parts(
                to_recognize_frame.data(0).as_ptr() as *const i16,
                to_recognize_frame.samples(),
            )
        };
        sub_title.source_buffer.extend(data);
        if sub_title.source_buffer.len() > 3200 {
            let buf = sub_title.source_buffer.drain(0..3200).collect::<Vec<i16>>();
            if let Ok(state) = sub_title.recognizer.accept_waveform(&buf) {
                let partial_result = sub_title.recognizer.partial_result();
                if !partial_result.partial.is_empty() {
                    let no_space_result = partial_result.partial.replace(" ", "");
                    sub_title.generated_str = no_space_result;
                }
                if let DecodingState::Finalized = state {
                    if let Some(res) = sub_title.recognizer.final_result().single() {
                        let no_space_result = res.text.replace(" ", "");
                        sub_title.generated_str = no_space_result;
                    }
                }
            }
        }
    }
    pub fn generated_str(&self) -> &str {
        &self.generated_str
    }
}
