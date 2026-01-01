use std::{collections::VecDeque, sync::Arc, time::Duration};

use rodio::Sink;
use tokio::{runtime::Handle, sync::RwLock, task::JoinHandle, time::sleep};
use tracing::{Instrument, Level, span};

use crate::{
    PlayerError, PlayerResult,
    ai_sub_title::{AISubTitle, UsedModel},
    decode::{MainStream, TinyDecoder},
};

pub struct AudioPlayer {
    sink: Arc<RwLock<rodio::Sink>>,
    _stream: rodio::OutputStream,
    current_volumn: f32,
    pts_que: Arc<RwLock<VecDeque<i64>>>,
    runtime_handle: Handle,
    _play_task_thread_handle: JoinHandle<()>,
}
impl AudioPlayer {
    pub fn new(
        runtime_handle: Handle,
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
    ) -> PlayerResult<Self> {
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .map_err(|e| PlayerError::Internal(e.to_string()))?;
        let sink = Arc::new(RwLock::new(rodio::Sink::connect_new(stream.mixer())));
        let pts_que = Arc::new(RwLock::new(VecDeque::new()));
        let to_move_sink = sink.clone();
        let to_move_pts_que = pts_que.clone();
        let play_task_thread_handle = runtime_handle.spawn(async move {
            let audio_play_span = span!(Level::INFO, "audio play");
            let _enter = audio_play_span.enter();
            AudioPlayer::play_task(
                tiny_decoder,
                to_move_sink,
                to_move_pts_que,
                used_model,
                ai_subtitle,
            )
            .in_current_span()
            .await
        });
        Ok(Self {
            sink,
            _stream: stream,
            current_volumn: 1.0,
            pts_que,
            runtime_handle,
            _play_task_thread_handle: play_task_thread_handle,
        })
    }
    async fn play_task(
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        sink: Arc<RwLock<Sink>>,
        pts_que: Arc<RwLock<VecDeque<i64>>>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
    ) {
        loop {
            sleep(Duration::from_millis(1)).await;
            /*
            add audio frame data to the audio player
             */
            {
                let mut tiny_decoder = tiny_decoder.write().await;
                let sink = sink.read().await;
                if let MainStream::Audio = tiny_decoder.main_stream() {
                    if sink.len() < 10 {
                        if let Some(audio_frame) = tiny_decoder.pull_one_audio_play_frame().await {
                            let mut pts_que = pts_que.write().await;
                            if let Some(pts) = audio_frame.pts() {
                                if pts_que.len() > 10 {
                                    pts_que.pop_front();
                                }
                                pts_que.push_back(pts);
                                AudioPlayer::play_raw_data_from_audio_frame(
                                    &sink,
                                    audio_frame.clone(),
                                )
                                .await;
                                let used_model = used_model.read().await;
                                let used_model_ref = &*used_model;
                                if let UsedModel::Chinese = used_model_ref {
                                    AISubTitle::push_frame_data(
                                        ai_subtitle.clone(),
                                        audio_frame,
                                        UsedModel::Chinese,
                                    )
                                    .await;
                                } else if let UsedModel::English = used_model_ref {
                                    AISubTitle::push_frame_data(
                                        ai_subtitle.clone(),
                                        audio_frame,
                                        UsedModel::English,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    async fn play_raw_data_from_audio_frame(
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
        let sink = self.sink.clone();
        let sink = self.runtime_handle.block_on(sink.read());
        sink.set_volume(volumn);
        self.current_volumn = volumn;
    }
    pub fn source_queue_skip_to_end(&mut self) {
        let sink = self.sink.clone();
        let sink = self.runtime_handle.block_on(sink.read());
        let pts_que = self.pts_que.clone();
        let mut pts_que = self.runtime_handle.block_on(pts_que.write());
        sink.clear();
        pts_que.clear();
    }
    pub fn pause(&self) {
        let sink = self.sink.clone();
        let sink = self.runtime_handle.block_on(sink.read());
        sink.pause();
    }
    pub fn play(&self) {
        let sink = self.sink.clone();
        let sink = self.runtime_handle.block_on(sink.read());
        sink.play();
    }
    pub fn last_source_pts(&self) -> PlayerResult<i64> {
        let pts_que = self.pts_que.clone();
        let pts_que = self.runtime_handle.block_on(pts_que.read());
        if !pts_que.is_empty() {
            Ok(pts_que[pts_que.len() - 1])
        } else {
            Err(PlayerError::Internal("audio source len is 0".to_string()))
        }
    }
    pub fn _len(&self) -> usize {
        let sink = self.sink.clone();
        let sink = self.runtime_handle.block_on(sink.read());
        sink.len()
    }
    pub fn current_volumn(&self) -> &f32 {
        &self.current_volumn
    }
}
