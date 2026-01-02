use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use ffmpeg_the_third::frame::Video;
use rodio::Sink;
use tokio::{
    runtime::Handle,
    sync::RwLock,
    task::{JoinHandle, yield_now},
};

use crate::{
    ai_sub_title::{AISubTitle, UsedModel},
    audio_play::AudioPlayer,
    decode::{MainStream, TinyDecoder},
};

pub struct PresentDataManager {
    _data_thread_handle: JoinHandle<()>,
}
impl PresentDataManager {
    pub fn new(
        runtime_handle: Handle,
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        current_video_frame: Arc<RwLock<Option<Video>>>,
        sink: Arc<Sink>,
        pts_que: Arc<RwLock<VecDeque<i64>>>,
        main_stream_current_timestamp: Arc<RwLock<i64>>,
        pause_flag: Arc<RwLock<bool>>,
    ) -> Self {
        Self {
            _data_thread_handle: runtime_handle.spawn(PresentDataManager::play_task(
                tiny_decoder,
                sink,
                pts_que,
                used_model,
                ai_subtitle,
                current_video_frame,
                main_stream_current_timestamp,
                pause_flag,
            )),
        }
    }
    async fn play_task(
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        sink: Arc<Sink>,
        pts_que: Arc<RwLock<VecDeque<i64>>>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        current_video_frame: Arc<RwLock<Option<Video>>>,
        main_stream_current_timestamp: Arc<RwLock<i64>>,
        pause_flag: Arc<RwLock<bool>>,
    ) {
        let mut change_instant = Instant::now();
        loop {
            yield_now().await;
            /*
            add audio frame data to the audio player
             */
            if !*pause_flag.read().await {
                let mut tiny_decoder = tiny_decoder.write().await;
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
                                    &*sink,
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
                if PresentDataManager::is_video_wait_for_audio(
                    &*tiny_decoder,
                    main_stream_current_timestamp.clone(),
                    current_video_frame.clone(),
                )
                .await
                {
                } else {
                    let ins_now = Instant::now();
                    if ins_now - change_instant > Duration::from_millis(0) {
                        if let Some(frame) = tiny_decoder.pull_one_video_play_frame().await {
                            let mut cur_frame = current_video_frame.write().await;
                            let main_stream = tiny_decoder.main_stream();
                            if let MainStream::Video = main_stream {
                                if let Some(f_pts) = frame.pts() {
                                    if let Some(cur_frame) = &mut *cur_frame {
                                        if let Some(cur_pts) = cur_frame.pts() {
                                            let time_base = tiny_decoder.video_time_base();
                                            if f_pts > 0
                                                && cur_pts > 0
                                                && ((f_pts - cur_pts)
                                                    * (*time_base).numerator() as i64
                                                    / (*time_base).denominator() as i64)
                                                    < 1
                                            {
                                                if let Some(ins) = change_instant.checked_add(
                                                    Duration::from_millis(
                                                        ((f_pts - cur_pts)
                                                            * 1000
                                                            * (*time_base).numerator() as i64
                                                            / (*time_base).denominator() as i64)
                                                            as u64,
                                                    ),
                                                ) {
                                                    change_instant = ins;
                                                }
                                            } else {
                                                change_instant = ins_now;
                                            }
                                        }
                                    }
                                }
                            } else if let MainStream::Audio = main_stream {
                                change_instant = ins_now;
                            }
                            *cur_frame = Some(frame);
                        }
                    }
                }
                PresentDataManager::update_current_timestamp(
                    main_stream_current_timestamp.clone(),
                    pts_que.clone(),
                    &tiny_decoder,
                    current_video_frame.clone(),
                )
                .await;
            }
        }
    }
    async fn update_current_timestamp(
        main_stream_current_timestamp: Arc<RwLock<i64>>,
        pts_que: Arc<RwLock<VecDeque<i64>>>,
        tiny_decoder: &TinyDecoder,
        current_video_frame: Arc<RwLock<Option<Video>>>,
    ) {
        /*
        add audio frame data to the audio player
         */
        let main_stream = tiny_decoder.main_stream();
        let mut main_ts = main_stream_current_timestamp.write().await;
        if let MainStream::Audio = main_stream {
            let pts_que = pts_que.read().await;
            if !pts_que.is_empty() {
                *main_ts = pts_que[pts_que.len() - 1];
            }
        } else if let MainStream::Video = main_stream {
            let cur_video_frame = current_video_frame.read().await;
            if let Some(frame) = &*cur_video_frame {
                if let Some(pts) = frame.pts() {
                    if pts > 0 {
                        *main_ts = pts;
                    }
                }
            }
        }
    }
    /// if video time-audio time is too high(more than 1 second),default return true
    async fn is_video_wait_for_audio(
        tiny_decoder: &TinyDecoder,
        main_stream_current_timestamp: Arc<RwLock<i64>>,
        current_video_frame: Arc<RwLock<Option<Video>>>,
    ) -> bool {
        if let MainStream::Video = tiny_decoder.main_stream() {
            return false;
        }
        let current_video_frame = current_video_frame.read().await;
        if let Some(frame) = &*current_video_frame {
            let timestamp = main_stream_current_timestamp.read().await;
            {
                let video_time_base = tiny_decoder.video_time_base();
                let audio_time_base = tiny_decoder.audio_time_base();
                if let Some(f_ts) = frame.pts() {
                    let v_time = f_ts * 1000 * video_time_base.numerator() as i64
                        / video_time_base.denominator() as i64;
                    let a_time = *timestamp * 1000 * audio_time_base.numerator() as i64
                        / audio_time_base.denominator() as i64;
                    let time_dur = v_time - a_time;
                    if time_dur > 0 {
                        // info!("wait audio v_time{},a_time{}", v_time, a_time);
                        return true;
                    }
                }
            }
        }

        false
    }
}
