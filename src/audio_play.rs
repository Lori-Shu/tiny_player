use std::collections::VecDeque;

use log::warn;

pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    _stream: Option<rodio::OutputStream>,
    current_volumn: f32,
    pts_vec: VecDeque<i64>,
}
impl AudioPlayer {
    pub fn new() -> Self {
        if let Ok(stream) = rodio::OutputStreamBuilder::open_default_stream() {
            Self {
                sink: Some(rodio::Sink::connect_new(stream.mixer())),
                _stream: Some(stream),
                current_volumn: 1.0,
                pts_vec: VecDeque::new(),
            }
        } else {
            warn!("rodio  open stream err");
            Self {
                sink: None,
                _stream: None,
                current_volumn: 1.0,
                pts_vec: VecDeque::new(),
            }
        }
    }
    pub fn play_raw_data_from_audio_frame(&self, audio_frame: ffmpeg_the_third::frame::Audio) {
        let audio_data = bytemuck::cast_slice::<u8, f32>(audio_frame.data(0));
        let audio_data =
            &audio_data[0..audio_frame.samples() * audio_frame.ch_layout().channels() as usize];
        let source = rodio::buffer::SamplesBuffer::new(
            audio_frame.ch_layout().channels() as u16,
            audio_frame.rate(),
            audio_data,
        );
        if let Some(sink) = &self.sink {
            sink.append(source);
        }
    }

    pub fn change_volumn(&mut self, volumn: f32) {
        if let Some(sink) = &self.sink {
            sink.set_volume(volumn);
            self.current_volumn = volumn;
        }
    }
    pub fn source_queue_skip_to_end(&mut self) {
        if let Some(sink) = &self.sink {
            sink.clear();
            self.pts_vec.clear();
            sink.play();
        }
    }
    pub fn pause_play(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }
    pub fn continue_play(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }
    pub fn push_pts(&mut self, pts: i64) {
        if self.pts_vec.len() > 30 {
            self.pts_vec.pop_front();
        }
        self.pts_vec.push_back(pts);
    }
    pub fn last_source_pts(&self) -> Result<i64, String> {
        if !self.pts_vec.is_empty() {
            Ok(self.pts_vec[self.pts_vec.len() - 1])
        } else {
            Err("audio source len is 0".to_string())
        }
    }
    pub fn len(&self) -> usize {
        if let Some(sink) = &self.sink {
            sink.len()
        } else {
            0
        }
    }
    pub fn current_volumn(&self) -> &f32 {
        &self.current_volumn
    }
}
