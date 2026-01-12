use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use ffmpeg_the_third::{ffi::av_frame_unref, frame::Video};
use rodio::Sink;
use tokio::{
    runtime::Handle,
    sync::{Notify, RwLock},
    task::JoinHandle,
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
        data_thread_notify: Arc<Notify>,
        runtime_handle: Handle,
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        current_video_frame: Arc<RwLock<Option<CurFrame>>>,
        sink: Arc<Sink>,
        main_stream_current_timestamp: Arc<RwLock<i64>>,
    ) -> Self {
        Self {
            _data_thread_handle: runtime_handle.spawn(PresentDataManager::play_task(
                data_thread_notify,
                tiny_decoder,
                sink,
                used_model,
                ai_subtitle,
                current_video_frame,
                main_stream_current_timestamp,
            )),
        }
    }
    async fn play_task(
        data_thread_notify: Arc<Notify>,
        tiny_decoder: Arc<RwLock<TinyDecoder>>,
        sink: Arc<Sink>,
        used_model: Arc<RwLock<UsedModel>>,
        ai_subtitle: Arc<RwLock<AISubTitle>>,
        current_video_frame: Arc<RwLock<Option<CurFrame>>>,
        main_stream_current_timestamp: Arc<RwLock<i64>>,
    ) {
        let mut change_instant = Instant::now();
        loop {
            /*
            add audio frame data to the audio player
             */
            {
                let mut audio_cur_ts = None;
                let mut tiny_decoder = tiny_decoder.write().await;
                if let MainStream::Audio = tiny_decoder.main_stream() {
                    if let Some(audio_frame) = tiny_decoder.pull_one_audio_play_frame().await {
                        if let Some(pts) = audio_frame.pts() {
                            audio_cur_ts = Some(pts);
                            AudioPlayer::play_raw_data_from_audio_frame(&sink, audio_frame.clone())
                                .await;
                            let used_model = used_model.read().await;
                            let used_model_ref = &*used_model;
                            let mut subtitle = ai_subtitle.write().await;
                            if UsedModel::Empty != *used_model_ref {
                                subtitle
                                    .push_frame_data(audio_frame, used_model_ref.clone())
                                    .await;
                            }
                        }
                    }
                }
                if !PresentDataManager::is_video_wait_for_audio(
                    &tiny_decoder,
                    main_stream_current_timestamp.clone(),
                    current_video_frame.clone(),
                )
                .await
                {
                    let ins_now = Instant::now();
                    if ins_now - change_instant > Duration::from_millis(0) {
                        if let Some(mut frame) = tiny_decoder.pull_one_video_play_frame().await {
                            let mut cur_frame = current_video_frame.write().await;
                            let main_stream = tiny_decoder.main_stream();
                            if let MainStream::Video = main_stream {
                                if let Some(f_pts) = frame.pts() {
                                    if let Some(cur_frame) = &mut *cur_frame {
                                        if let Some(cur_pts) = cur_frame.av_frame().pts() {
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
                            let data =
                                TinyDecoder::convert_frame_data_to_no_padding_layout(&mut frame)
                                    .await;
                            let pts = frame.pts();
                            // free the useless data
                            unsafe {
                                av_frame_unref(frame.as_mut_ptr());
                            }
                            frame.set_pts(pts);
                            *cur_frame = Some(CurFrame::new(frame, data));
                        }
                    }
                }
                PresentDataManager::update_current_timestamp(
                    main_stream_current_timestamp.clone(),
                    audio_cur_ts,
                    &tiny_decoder,
                    current_video_frame.clone(),
                )
                .await;
            }
            data_thread_notify.notified().await;
        }
    }
    async fn update_current_timestamp(
        main_stream_current_timestamp: Arc<RwLock<i64>>,
        audio_pts: Option<i64>,
        tiny_decoder: &TinyDecoder,
        current_video_frame: Arc<RwLock<Option<CurFrame>>>,
    ) {
        /*
        add audio frame data to the audio player
         */
        let main_stream = tiny_decoder.main_stream();
        let mut main_ts = main_stream_current_timestamp.write().await;
        if let MainStream::Audio = main_stream {
            if let Some(pts) = audio_pts {
                *main_ts = pts;
            }
        } else if let MainStream::Video = main_stream {
            let cur_video_frame = current_video_frame.read().await;
            if let Some(frame) = &*cur_video_frame {
                if let Some(pts) = frame.av_frame().pts() {
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
        current_video_frame: Arc<RwLock<Option<CurFrame>>>,
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
                if let Some(f_ts) = frame.av_frame().pts() {
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
pub struct CurFrame {
    frame: Video,
    image_data: Box<[u8]>,
}
impl CurFrame {
    pub fn new(frame: Video, image_data: Box<[u8]>) -> Self {
        Self { frame, image_data }
    }
    pub fn av_frame(&self) -> &Video {
        &self.frame
    }
    pub fn data(&self) -> &[u8] {
        &self.image_data
    }
}
