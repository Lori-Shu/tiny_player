use std::time::Duration;

use log::warn;

pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    handle: Option<rodio::OutputStreamHandle>,
    time_stamp_vec:Vec<i64>,
}
impl AudioPlayer {
    pub fn new() -> Self {
        return Self {
            stream: None,
            handle: None,
            sink: None,
            time_stamp_vec:vec![]
        };
    }
    pub fn init_device(&mut self) {
        let (stream, handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&handle).unwrap();
        self.sink = Some(sink);
        self.handle = Some(handle);
        self.stream = Some(stream);
    }
    pub fn play_raw_data_from_audio_frame(&self, audio_frame: ffmpeg_next::frame::Audio) {
        let audio_data: &[i16] = bytemuck::cast_slice(audio_frame.data(0));
        let source = rodio::buffer::SamplesBuffer::new(
            audio_frame.channels(),
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
    pub fn sync_play_time(&self,){
        let sink = self.sink.as_ref().unwrap();
            if sink.len() > 30 {
                // warn!("pos:{}",millis);
                sink.set_speed(1.05);
            }
            if sink.len() <10{
                sink.set_speed(1.0);
            }
    }
}
