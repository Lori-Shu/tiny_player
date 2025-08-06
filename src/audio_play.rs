use log::warn;

pub struct AudioPlayer {
    sink: Option<rodio::Sink>,
    stream: Option<rodio::OutputStream>,
    handle: Option<rodio::OutputStreamHandle>,
    pub current_volumn: f32,
    pts_vec2: Vec<i64>,
}
impl AudioPlayer {
    pub fn new() -> Self {
        return Self {
            pts_vec2: vec![0; 10],
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
    /*
    one audio source time is about 21 millisecond,if the len is beyond about 10 source,
    skip one to catch the video stime
     */
    pub fn sync_play_time(&self, a_strt: i64, a_base: i64, v_strt: i64, v_base: i64, v_pts: i64) {
        let sink = self.sink.as_ref().unwrap();
        let a_mill = (a_strt + self.pts_vec2[0]) * 1000 / a_base;
        let v_mill = (v_strt + v_pts) * 1000 / v_base;
        warn!("a:{}||||v:{}", a_mill, v_mill);
        // if a_mill + 100 > v_mill {
        //     sink.set_speed(1.0);

        // } else {
        //     sink.set_speed(1.4);
        // }
        if a_mill + 100 < v_mill {
            sink.skip_one();
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
    pub fn pause_play(&self) {
        self.sink.as_ref().unwrap().pause();
    }
    pub fn continue_play(&self) {
        self.sink.as_ref().unwrap().play();
    }
    pub fn set_pts(&mut self, pts: i64) {
        if self.pts_vec2.len() > 10 {
            self.pts_vec2.remove(0);
        }
        self.pts_vec2.push(pts);
    }
    pub fn get_pts(&self) -> i64 {
        self.pts_vec2[1]
    }
    pub fn len(&self) -> usize {
        self.sink.as_ref().unwrap().len()
    }
}
