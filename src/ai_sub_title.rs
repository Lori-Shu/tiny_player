use ffmpeg_the_third::{ChannelLayout, format::Sample, frame::Audio};
use log::warn;
use vosk::{DecodingState, Model, Recognizer};

use crate::{PlayerError, PlayerResult};

pub struct AISubTitle {
    _recognize_model: Model,
    recognizer: Recognizer,
}
impl AISubTitle {
    pub fn new() -> PlayerResult<Self> {
        let current_exe_path = std::env::current_exe()
            .map_err(|_e| PlayerError::Internal("exe path get err".to_string()))?;
        if let Some(folder_path) = current_exe_path.parent() {
            let model_path = folder_path.join("vosk-model-small-cn-0.22");
            if let Some(model_path_str) = model_path.to_str() {
                if let Some(recognize_model) = Model::new(model_path_str) {
                    if let Some(mut rez) = Recognizer::new(&recognize_model, 16000.0) {
                        rez.set_max_alternatives(10);
                        rez.set_words(true);
                        rez.set_partial_words(true);
                        return Ok(Self {
                            _recognize_model: recognize_model,
                            recognizer: rez,
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
            // unsafe {
            //     let mut swr_ctx = swr_alloc();
            //     let mut r = swr_alloc_set_opts2(
            //         &mut swr_ctx,
            //         ChannelLayout::MONO.as_ptr(),
            //         Sample::I16(ffmpeg_the_third::util::format::sample::Type::Packed).into(),
            //         16000,
            //         audio_frame.ch_layout().as_ptr(),
            //         audio_frame.format().into(),
            //         audio_frame.rate() as i32,
            //         0,
            //         null_mut(),
            //     );
            //     if r < 0 {
            //         warn!("swr_alloc_set_opts2 err");
            //     }
            //     let mut buffer=vec![0_u8;1024];
            //     swr_convert(swr_ctx, & buffer.as_mut_ptr(), 1024, audio_frame.data(0), audio_frame.data(0).len());
            //     swr_free(&mut swr_alloc);
            // }
            let mut to_recognize_frame = Audio::empty();
            if resampler.run(&audio_frame, &mut to_recognize_frame).is_ok() {
                if let Ok(data) = bytemuck::try_cast_slice::<u8, i16>(to_recognize_frame.data(0)) {
                    if let Ok(state) = self.recognizer.accept_waveform(data) {
                        if let DecodingState::Finalized = state {
                            warn!("recognized words: {:?}", self.recognizer.partial_result());
                        }
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod test {
    use std::path::Path;

    use vosk::{Model, Recognizer};

    #[test]
    fn test_vosk_api() {
        // Simplified version of examples/read_wav.rs
        let mut reader = hound::WavReader::open(Path::new("D:/DownLoads/for-test.wav")).unwrap();
        let _wav_spec = reader.spec();

        let samples = reader
            .samples::<i16>()
            .map(|a| a.unwrap())
            .collect::<Vec<i16>>();
        // Normally you would not want to hardcode the audio samples
        let model_path = "D:/rustprojects/tiny_player/resources/model/vosk-model-small-cn-0.22";

        let model = Model::new(model_path).unwrap();
        let mut recognizer = Recognizer::new(&model, 16000.0).unwrap();

        recognizer.set_max_alternatives(10);
        recognizer.set_words(true);
        recognizer.set_partial_words(true);

        for sample in samples.chunks(100) {
            recognizer.accept_waveform(sample).unwrap();
            println!("{:#?}", recognizer.partial_result());
        }

        // println!("{:#?}", recognizer.final_result().multiple().unwrap());
    }
}
