use ffmpeg_the_third::{ChannelLayout, format::Sample, frame::Audio};
use log::warn;
use vosk::{DecodingState, Model, Recognizer};

use crate::{CURRENT_EXE_PATH, PlayerError, PlayerResult};

pub struct AISubTitle {
    _recognize_model: Model,
    recognizer: Recognizer,
    generated_str: String,
}
impl AISubTitle {
    pub fn new() -> PlayerResult<Self> {
        let exe_path = CURRENT_EXE_PATH;
        let current_exe_path = exe_path.as_ref().map_err(|e| e.clone())?;
        if let Some(folder_path) = current_exe_path.parent() {
            let model_path = folder_path.join("model/vosk-model-small-cn-0.22");
            if let Some(model_path_str) = model_path.to_str() {
                if let Some(recognize_model) = Model::new(model_path_str) {
                    if let Some(mut rez) = Recognizer::new(&recognize_model, 16000.0) {
                        rez.set_max_alternatives(0);
                        rez.set_words(true);
                        rez.set_nlsml(true);
                        return Ok(Self {
                            _recognize_model: recognize_model,
                            recognizer: rez,
                            generated_str: String::new(),
                        });
                    }
                }
            }
        }
        Err(PlayerError::Internal(
            "AISubTitle construct err".to_string(),
        ))
    }

    pub fn push_frame_data(&mut self, audio_frame: ffmpeg_the_third::frame::Audio) {
        if let Ok(mut resampler) = ffmpeg_the_third::software::resampler2(
            (
                audio_frame.format(),
                audio_frame.ch_layout(),
                audio_frame.rate(),
            ),
            (
                Sample::I16(ffmpeg_the_third::util::format::sample::Type::Packed),
                ChannelLayout::MONO,
                16000,
            ),
        ) {
            let mut to_recognize_frame = Audio::empty();
            if resampler.run(&audio_frame, &mut to_recognize_frame).is_ok() {
                let data: &[i16] = unsafe {
                    std::slice::from_raw_parts(
                        to_recognize_frame.data(0).as_ptr() as *const i16,
                        to_recognize_frame.samples(),
                    )
                };
                if let Ok(state) = self.recognizer.accept_waveform(data) {
                    if let DecodingState::Finalized = state {
                        let final_result = self.recognizer.final_result();
                        if let Some(result) = final_result.single() {
                            let trimed_result = result.text.replace(" ", "");
                            self.generated_str.push_str(&trimed_result);
                            warn!(
                                "sub str len{} content:{} ",
                                self.generated_str.len(),
                                self.generated_str
                            );
                        }
                    }
                }
                if self.generated_str.chars().count() > 20 {
                    self.generated_str.remove(0);
                }
            }
        }
    }
    pub fn generated_str(&self) -> &str {
        &self.generated_str
    }
}
