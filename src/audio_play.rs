pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    handle: Option<rodio::OutputStreamHandle>,
    pub current_volumn: f32,
    pts_vec: Vec<i64>,
}
impl AudioPlayer {
    pub fn new() -> Self {
        return Self {
            pts_vec: vec![],
            current_volumn: 1.0,
            stream: None,
            handle: None,
            sink: None,
        };
    }
    pub fn init_device(&mut self) {
        let (stream, handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&handle).unwrap();
        self.sink = Some(sink);
        self.handle = Some(handle);
        self.stream = Some(stream);
    }
    pub fn play_raw_data_from_audio_frame(&self, audio_frame: ffmpeg_the_third::frame::Audio) {
        let audio_data: &[i16] = bytemuck::cast_slice(audio_frame.data(0));
        let source = rodio::buffer::SamplesBuffer::new(
            audio_frame.ch_layout().channels() as u16,
            audio_frame.rate(),
            audio_data,
        );

        let sink = self.sink.as_ref().unwrap();
        sink.append(source);
    }

    pub fn change_volumn(&self) {
        let sink = self.sink.as_ref().unwrap();
        sink.set_volume(self.current_volumn);
    }
    pub fn source_queue_skip_to_end(&mut self) {
        let sink = self.sink.as_ref().unwrap();
        sink.clear();
        self.pts_vec.clear();
        sink.play();
    }
    pub fn pause_play(&self) {
        self.sink.as_ref().unwrap().pause();
    }
    pub fn continue_play(&self) {
        self.sink.as_ref().unwrap().play();
    }
    pub fn set_pts(&mut self, pts: i64) {
        if self.pts_vec.len() > 10 {
            self.pts_vec.remove(0);
        }
        self.pts_vec.push(pts);
    }
    pub fn get_last_source_pts(&self) -> i64 {
        self.pts_vec[self.pts_vec.len() - 1]
    }
    pub fn len(&self) -> usize {
        self.sink.as_ref().unwrap().len()
    }
}
