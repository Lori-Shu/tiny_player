pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    pub current_volumn: f32,
    pts_vec: Vec<i64>,
}
impl AudioPlayer {
    pub fn new() -> Self {
        let mut sel = Self {
            pts_vec: vec![],
            current_volumn: 1.0,
            stream: None,
            sink: None,
        };
        if let Ok(stream) = rodio::OutputStreamBuilder::open_default_stream() {
            let sink = rodio::Sink::connect_new(stream.mixer());
            sel.sink = Some(sink);
            sel.stream = Some(stream);
        }
        sel
    }
    pub fn play_raw_data_from_audio_frame(&self, audio_frame: ffmpeg_the_third::frame::Audio) {
        let audio_data: &[f32] = bytemuck::cast_slice(audio_frame.data(0));
        let source = rodio::buffer::SamplesBuffer::new(
            audio_frame.ch_layout().channels() as u16,
            audio_frame.rate(),
            audio_data,
        );
        if let Some(sink) = &self.sink {
            sink.append(source);
        }
    }

    pub fn change_volumn(&self) {
        if let Some(sink) = &self.sink {
            sink.set_volume(self.current_volumn);
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
    pub fn set_pts(&mut self, pts: i64) {
        if self.pts_vec.len() > 30 {
            self.pts_vec.remove(0);
        }
        self.pts_vec.push(pts);
    }
    pub fn get_last_source_pts(&self) -> Result<i64, String> {
        if !self.pts_vec.is_empty() {
            return Ok(self.pts_vec[self.pts_vec.len() - 1]);
        } else {
            return Err("audio source len is 0".to_string());
        }
    }
    pub fn len(&self) -> usize {
        if let Some(sink) = &self.sink {
            sink.len()
        } else {
            0
        }
    }
}
