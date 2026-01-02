use std::{collections::VecDeque, sync::Arc};

use rodio::Sink;
use tokio::{runtime::Handle, sync::RwLock};

use crate::{PlayerError, PlayerResult};

pub struct AudioPlayer {
    sink: Arc<rodio::Sink>,
    _stream: rodio::OutputStream,
    current_volumn: f32,
    pts_que: Arc<RwLock<VecDeque<i64>>>,
    runtime_handle: Handle,
}
impl AudioPlayer {
    pub fn new(runtime_handle: Handle) -> PlayerResult<Self> {
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|e| PlayerError::Internal(e.to_string()))?;
        let sink = Arc::new(rodio::Sink::connect_new(stream.mixer()));
        let pts_que = Arc::new(RwLock::new(VecDeque::new()));

        Ok(Self {
            sink,
            _stream: stream,
            current_volumn: 1.0,
            pts_que,
            runtime_handle,
        })
    }

    pub async fn play_raw_data_from_audio_frame(
        sink: &Sink,
        audio_frame: ffmpeg_the_third::frame::Audio,
    ) {
        let audio_data = bytemuck::cast_slice::<u8, f32>(audio_frame.data(0));
        let audio_data =
            &audio_data[0..audio_frame.samples() * audio_frame.ch_layout().channels() as usize];
        let source = rodio::buffer::SamplesBuffer::new(
            audio_frame.ch_layout().channels() as u16,
            audio_frame.rate(),
            audio_data,
        );
        sink.append(source);
    }

    pub fn change_volumn(&mut self, volumn: f32) {
        self.sink.set_volume(volumn);
        self.current_volumn = volumn;
    }
    pub fn source_queue_skip_to_end(&mut self) {
        let pts_que = self.pts_que.clone();
        let mut pts_que = self.runtime_handle.block_on(pts_que.write());
        self.sink.clear();
        pts_que.clear();
    }
    pub fn pause(&self) {
        self.sink.pause();
    }
    pub fn play(&self) {
        self.sink.play();
    }
    pub fn _last_source_pts(&self) -> PlayerResult<i64> {
        let pts_que = self.pts_que.clone();
        let pts_que = self.runtime_handle.block_on(pts_que.read());
        if !pts_que.is_empty() {
            Ok(pts_que[pts_que.len() - 1])
        } else {
            Err(PlayerError::Internal("audio source len is 0".to_string()))
        }
    }
    pub fn _len(&self) -> usize {
        self.sink.len()
    }
    pub fn current_volumn(&self) -> &f32 {
        &self.current_volumn
    }
    pub fn sink(&self) -> Arc<Sink> {
        self.sink.clone()
    }
    pub fn pts_que(&self) -> Arc<RwLock<VecDeque<i64>>> {
        self.pts_que.clone()
    }
}
