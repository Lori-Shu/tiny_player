use std::time::Duration;

use log::warn;

pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    handle: Option<rodio::OutputStreamHandle>,
    time_stamp_vec: Vec<i64>,
    pub current_volumn: f32,
}
impl AudioPlayer {
    pub fn new() -> Self {
        return Self {
            current_volumn: 1.0,
            stream: None,
            handle: None,
            sink: None,
            time_stamp_vec: vec![],
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
    /*
    one audio source time is about 21 millisecond,if the len is beyond about 10 source,
    skip one to catch the video stime
     */
    pub fn sync_play_time(&self) {
        let sink = self.sink.as_ref().unwrap();
        if sink.len() < 10 {
            sink.set_speed(1.0);
        } else {
            // warn!("pos:{}",millis);
            sink.set_speed(1.05);
        }
    }
    pub fn change_volumn(&self) {
        let sink = self.sink.as_ref().unwrap();
        sink.set_volume(self.current_volumn);
    }
    pub fn source_queue_skip_to_end(&self) {
        let sink = self.sink.as_ref().unwrap();
        sink.clear();
        sink.play();
    }
    pub fn pause_play(&self){
        self.sink.as_ref().unwrap().pause();
    }
    pub fn continue_play(&self){
        self.sink.as_ref().unwrap().play();
    }
}
