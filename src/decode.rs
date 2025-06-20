use std::{
    path::Path,
    sync::{Arc, Mutex, RwLock},
    thread::{self, sleep},
    time::Duration,
};

use ffmpeg_the_third::{
    codec::Context,
    format::{self, Pixel},
    frame::Video,
    media::Type,
};
use log::warn;
use time::format_description;

pub struct TinyDecoder {
    video_stream_index: usize,
    audio_stream_index: usize,
    frame_time_base: i32,
    video_frame_rate: i32,
    total_video_frames: i64,
    total_video_time_formatted_string: String,
    format_input: Option<Arc<Mutex<ffmpeg_the_third::format::context::Input>>>,
    video_decoder: Option<Arc<Mutex<ffmpeg_the_third::decoder::Video>>>,
    audio_decoder: Option<Arc<Mutex<ffmpeg_the_third::decoder::Audio>>>,
    scaler_ctx: Option<ffmpeg_the_third::software::scaling::Context>,
    resampler_ctx: Option<ffmpeg_the_third::software::resampling::Context>,
    video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
    audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
    packet_demux_thread_handler: Option<thread::JoinHandle<()>>,
    packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
    process_change_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    frame_decode_thread_handler: Option<thread::JoinHandle<()>>,
    frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    current_play_timestamp:Arc<Mutex<i64>>
}
impl TinyDecoder {
    pub fn new(current_ts:Arc<Mutex<i64>>) -> Self {
        ffmpeg_the_third::init().unwrap();
        return Self {
            video_stream_index: 0,
            audio_stream_index: 0,
            frame_time_base: 0,
            video_frame_rate: 0,
            total_video_frames: 0,
            total_video_time_formatted_string: String::new(),
            format_input: None,
            video_decoder: None,
            audio_decoder: None,
            scaler_ctx: None,
            resampler_ctx: None,
            video_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            audio_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            packet_demux_thread_handler: None,
            packet_demux_thread_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            process_change_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            frame_decode_thread_handler: None,
            frame_decode_thread_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            packet_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            current_play_timestamp:current_ts,
        };
    }

    pub fn set_file_path_and_init_par(&mut self, file_path: &Path) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let mut format_input = format::input(file_path).unwrap();
        let video_stream = format_input.streams().best(Type::Video).unwrap();
        let audio_stream = format_input.streams().best(Type::Audio).unwrap();
        self.video_stream_index = video_stream.index();
        self.audio_stream_index = audio_stream.index();
        let v_frames = video_stream.frames();
        let fps = video_stream.avg_frame_rate().0;
        {
            let sec_num = v_frames / fps as i64;
            let sec = (sec_num % 60) as u8;
            let min_num = sec_num / 60;
            let min = (min_num % 60) as u8;
            let hour_num = min_num / 60;
            let hour = hour_num as u8;
            let time = time::Time::from_hms(hour, min, sec).unwrap();
            let formatter = format_description::parse("[hour]:[minute]:[second]").unwrap();
            self.total_video_time_formatted_string = time.format(&formatter).unwrap();
        }
        self.total_video_frames = v_frames;
        let video_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(video_stream.parameters()).unwrap();
        let audio_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(audio_stream.parameters()).unwrap();
        let mut video_decoder = video_decoder_ctx.decoder().video().unwrap();
        warn!(
            "video decoder width=={} height=={}",
            video_decoder.width(),
            video_decoder.height()
        );
        self.frame_time_base = video_stream.time_base().1;
        warn!("time_base==={}", self.frame_time_base);
        self.video_frame_rate = fps;
        warn!("video_frame_rate==={}", self.video_frame_rate);
        self.scaler_ctx = Some(
            ffmpeg_the_third::software::converter(
                (video_decoder.width(), video_decoder.height()),
                video_decoder.format(),
                Pixel::RGBA,
            )
            .unwrap(),
        );
        let mut audio_decoder = audio_decoder_ctx.decoder().audio().unwrap();
        self.resampler_ctx = Some(
            ffmpeg_the_third::software::resampler2(
                (
                    audio_decoder.format(),
                    audio_decoder.ch_layout(),
                    audio_decoder.rate(),
                ),
                (
                    format::Sample::I16(format::sample::Type::Packed),
                    audio_decoder.ch_layout(),
                    48000,
                ),
            )
            .unwrap(),
        );
        self.video_decoder = Some(Arc::new(Mutex::new(video_decoder)));
        self.audio_decoder = Some(Arc::new(Mutex::new(audio_decoder)));
        self.format_input = Some(Arc::new(Mutex::new(format_input)));
    }
    fn packet_demux_process(
        packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        format_input: Arc<Mutex<ffmpeg_the_third::format::context::Input>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        process_change_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        current_timestamp: std::sync::Arc<std::sync::Mutex<i64>>,
    ) {
        loop {
            if packet_demux_thread_stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let mut cache_len = 0;
            {
                let rw_lock_read_guard = packet_cache_vec.read().unwrap();
                cache_len = rw_lock_read_guard.len();
            }
            if cache_len >= 100  {
                sleep(Duration::from_millis(10));
            }
            {
                let mut lock_guard = format_input.lock().unwrap();

                if let Ok((stream, packet)) = lock_guard.packets().next().unwrap() {
                    {
                        let mut rw_lock_write_guard1 = packet_cache_vec.write().unwrap();
                        rw_lock_write_guard1.push(packet);
                    }
                }
            }
        }
    }
    fn frame_decode_process(
        format_input: Arc<Mutex<ffmpeg_the_third::format::context::Input>>,
        video_stream_index: Arc<usize>,
        audio_stream_index: Arc<usize>,
        video_decoder: Arc<Mutex<ffmpeg_the_third::decoder::Video>>,
        audio_decoder: Arc<Mutex<ffmpeg_the_third::decoder::Audio>>,
        video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
        audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        process_change_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        current_timestamp: std::sync::Arc<std::sync::Mutex<i64>>,
    ) {
        loop {
            if frame_decode_thread_stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let mut frame_vec_len = 0;
            {
                let mut guard = video_frame_cache_vec.read().unwrap();
                frame_vec_len = guard.len();
            }
            if frame_vec_len >= 100  {
                sleep(Duration::from_millis(10));
            }
            let mut front_packet = ffmpeg_the_third::Packet::empty();
            {
                let mut rw_lock_write_guard = packet_cache_vec.write().unwrap();
                if rw_lock_write_guard.len() > 0 {
                    front_packet = rw_lock_write_guard.remove(0);
                }
            }
                if front_packet.stream() == *video_stream_index {
                    {
                        let mut lock_guard = video_decoder.lock().unwrap();
                        lock_guard.send_packet(&front_packet).unwrap();

                        loop {
                            let mut video_frame_tmp = ffmpeg_the_third::frame::Video::empty();
                            if let Err(e) = lock_guard.receive_frame(&mut video_frame_tmp) {
                                break;
                            }
                            if process_change_flag.load(std::sync::atomic::Ordering::Acquire) {
                                
                                warn!("throw frame ts==={}",video_frame_tmp.timestamp().unwrap());
                                {
                                        let mut ts_mutex_guard = current_timestamp.lock().unwrap();
                                        if video_frame_tmp.timestamp().unwrap()>*ts_mutex_guard{
                                                *ts_mutex_guard=video_frame_tmp.timestamp().unwrap();
                                                process_change_flag.store(false, std::sync::atomic::Ordering::Release);
                                        }
                                }
                                
                            }else{
                                // warn!("decode video frame ts:{}",video_frame_tmp.timestamp().unwrap());
                                // warn!("decode video frame pts:{}",video_frame_tmp.pts().unwrap());
                                let mut rw_lock_write_guard1 =
                                    video_frame_cache_vec.write().unwrap();
                                rw_lock_write_guard1.push(video_frame_tmp);
                            }
                        }
                    }
                } else if front_packet.stream() == *audio_stream_index {
                    {
                        let mut lock_guard = audio_decoder.lock().unwrap();
                        lock_guard.send_packet(&front_packet).unwrap();
                        loop {
                            let mut audio_frame_tmp = ffmpeg_the_third::frame::Audio::empty();
                            if let Err(e) = lock_guard.receive_frame(&mut audio_frame_tmp) {
                                break;
                            }
                                if !process_change_flag.load(std::sync::atomic::Ordering::Acquire){
                                let mut rw_lock_write_guard1 =
                                    audio_frame_cache_vec.write().unwrap();
                                rw_lock_write_guard1.push(audio_frame_tmp);
                            }
                        }
                    }
                }
        }
    }
    pub fn start_process_threads(&mut self) {
        let mut packet_demux_thread_stop_flag = self.packet_demux_thread_stop_flag.clone();
        let mut format_input = self.format_input.as_ref().unwrap().clone();
        let mut packet_cache_vec = self.packet_cache_vec.clone();
        let mut process_change_flag = self.process_change_flag.clone();
        let mut process_change_target_timestamp = self.current_play_timestamp.clone();

        self.packet_demux_thread_handler = Some(thread::spawn(move || {
            Self::packet_demux_process(
                packet_demux_thread_stop_flag,
                format_input,
                packet_cache_vec,
                process_change_flag,
                process_change_target_timestamp,
            );
        }));
        let mut format_input = self.format_input.as_ref().unwrap().clone();
        let mut video_stream_index = Arc::new(self.video_stream_index);
        let mut audio_stream_index = Arc::new(self.audio_stream_index);
        let mut video_decoder = self.video_decoder.as_ref().unwrap().clone();
        let mut audio_decoder = self.audio_decoder.as_ref().unwrap().clone();
        let mut video_frame_cache_vec = self.video_frame_cache_vec.clone();
        let mut audio_frame_cache_vec = self.audio_frame_cache_vec.clone();
        let mut packet_cache_vec = self.packet_cache_vec.clone();
        let mut frame_decode_thread_stop_flag = self.frame_decode_thread_stop_flag.clone();
        let mut process_change_flag = self.process_change_flag.clone();
        let mut process_change_target_timestamp = self.current_play_timestamp.clone();
        self.frame_decode_thread_handler = Some(std::thread::spawn(move || {
            Self::frame_decode_process(
                format_input,
                video_stream_index,
                audio_stream_index,
                video_decoder,
                audio_decoder,
                video_frame_cache_vec,
                audio_frame_cache_vec,
                packet_cache_vec,
                frame_decode_thread_stop_flag,
                process_change_flag,
                process_change_target_timestamp
            );
        }));
    }
    pub fn get_one_audio_play_frame(&mut self) -> Option<ffmpeg_the_third::frame::Audio> {
        let resampler_ctx = self.resampler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Audio::empty();
        let mut raw_frame = ffmpeg_the_third::frame::Audio::empty();
        {
            let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                raw_frame = rw_lock_write_guard.remove(0);
            } else {
                return None;
            }
        }
        resampler_ctx.run(&raw_frame, &mut res).unwrap();
        return Some(res);
    }
    pub fn get_one_video_play_frame(&mut self) -> Option<ffmpeg_the_third::frame::Video> {
        let scaler_ctx = self.scaler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Video::empty();
        let mut return_val = None;
        {
            let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                let raw_frame = rw_lock_write_guard.remove(0);
                scaler_ctx.run(&raw_frame, &mut res).unwrap();

                return_val = Some(res);
            }
        }
        return return_val;
    }
    pub fn get_input_par(&self) -> Option<Arc<Mutex<ffmpeg_the_third::format::context::Input>>> {
        if self.format_input.is_none() {
            return None;
        }
        return Some(self.format_input.as_ref().unwrap().clone());
    }
    pub fn get_resampler(&self) -> &ffmpeg_the_third::software::resampling::Context {
        return self.resampler_ctx.as_ref().unwrap();
    }
    pub fn get_time_base(&self) -> i32 {
        return self.frame_time_base;
    }
    pub fn get_video_frame_rate(&self) -> i32 {
        return self.video_frame_rate;
    }
    pub fn get_total_video_frames(&self) -> i64 {
        return self.total_video_frames;
    }
    pub fn get_total_video_time_formatted_string(&self) -> &String {
        return &self.total_video_time_formatted_string;
    }
    pub fn seek_timestamp_to_decode(&self,ts:i64) {
        {
                {
                    let mut lock_guard = self.format_input.as_ref().unwrap().lock().unwrap();
                    let time_base = lock_guard
                        .streams()
                        .best(Type::Video)
                        .unwrap()
                        .time_base()
                        .1 as i64;
                    lock_guard
                        .seek(ts, ..)
                        .unwrap();
                }
                {
                        let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
                        rw_lock_write_guard.clear();
                }
                {
                        let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
                        rw_lock_write_guard.clear();
                }
            }
        self.process_change_flag
            .store(true, std::sync::atomic::Ordering::Release);
    }
}
