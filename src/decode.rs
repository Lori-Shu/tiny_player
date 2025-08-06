use std::{
    path::Path,
    sync::{Arc, Mutex, RwLock, atomic::AtomicBool},
    thread::{self, sleep},
    time::Duration,
};

use ffmpeg_the_third::{
    format::{self, Pixel},
    media::Type,
};
use log::warn;
use time::format_description;

pub struct TinyDecoder {
    video_stream_index: Arc<Mutex<usize>>,
    audio_stream_index: Arc<Mutex<usize>>,
    video_time_base: i32,
    audio_time_base: i32,
    video_start_time: i64,
    audio_start_time: i64,
    video_frame_rate: i32,
    video_frame_rect: [u32; 2],
    end_video_ts: i64,
    total_video_time_formatted_string: String,
    format_input: Option<Arc<Mutex<ffmpeg_the_third::format::context::Input>>>,
    input_play_end_flag: Arc<AtomicBool>,
    video_decoder: Option<Arc<Mutex<ffmpeg_the_third::decoder::Video>>>,
    audio_decoder: Option<Arc<Mutex<ffmpeg_the_third::decoder::Audio>>>,
    converter_ctx: Option<ffmpeg_the_third::software::scaling::Context>,
    resampler_ctx: Option<ffmpeg_the_third::software::resampling::Context>,
    video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
    audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
    packet_demux_thread_handler: Option<thread::JoinHandle<()>>,
    packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
    frame_decode_thread_handler: Option<thread::JoinHandle<()>>,
    frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl TinyDecoder {
    pub fn new() -> Self {
        ffmpeg_the_third::init().unwrap();
        return Self {
            video_stream_index: Arc::new(Mutex::new(0)),
            audio_stream_index: Arc::new(Mutex::new(0)),
            video_time_base: 0,
            audio_time_base: 0,
            video_start_time: 0,
            audio_start_time: 0,
            video_frame_rate: 0,
            video_frame_rect: [0, 0],
            end_video_ts: 0,
            total_video_time_formatted_string: String::new(),
            format_input: None,
            input_play_end_flag: Arc::new(AtomicBool::new(false)),
            video_decoder: None,
            audio_decoder: None,
            converter_ctx: None,
            resampler_ctx: None,
            video_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            audio_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            packet_demux_thread_handler: None,
            packet_demux_thread_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            frame_decode_thread_handler: None,
            frame_decode_thread_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            packet_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
        };
    }

    pub fn set_file_path_and_init_par(&mut self, file_path: &Path) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let format_input = format::input(file_path).unwrap();
        let video_stream = format_input.streams().best(Type::Video).unwrap();
        let audio_stream = format_input.streams().best(Type::Audio).unwrap();
        self.audio_start_time = audio_stream.start_time();
        self.video_start_time = video_stream.start_time();
        {
            let mut mutex_guard = self.video_stream_index.lock().unwrap();
            *mutex_guard = video_stream.index();
        }
        {
            let mut mutex_guard = self.audio_stream_index.lock().unwrap();
            *mutex_guard = audio_stream.index();
        }
        let v_frames = video_stream.frames();
        let fps = video_stream.avg_frame_rate().0;
        self.video_time_base = video_stream.time_base().1;
        warn!("video time_base==={}", self.video_time_base);
        self.video_frame_rate = fps;
        warn!("video_frame_rate==={}", self.video_frame_rate);
        self.audio_time_base = audio_stream.time_base().1;
        warn!("audio time_base==={}", self.audio_time_base);
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
        self.end_video_ts = v_frames / fps as i64 * self.video_time_base as i64;
        warn!("video end ts:{}", self.end_video_ts);
        let video_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(video_stream.parameters()).unwrap();
        let audio_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(audio_stream.parameters()).unwrap();
        let video_decoder = video_decoder_ctx.decoder().video().unwrap();
        warn!(
            "video decoder width=={} height=={}",
            video_decoder.width(),
            video_decoder.height()
        );
        self.converter_ctx = Some(
            ffmpeg_the_third::software::converter(
                (video_decoder.width(), video_decoder.height()),
                video_decoder.format(),
                Pixel::RGBA,
            )
            .unwrap(),
        );
        self.video_frame_rect = [video_decoder.width(), video_decoder.height()];
        let audio_decoder = audio_decoder_ctx.decoder().audio().unwrap();
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
                    audio_decoder.rate(),
                ),
            )
            .unwrap(),
        );
        self.input_play_end_flag
            .store(false, std::sync::atomic::Ordering::Release);
        self.video_decoder = Some(Arc::new(Mutex::new(video_decoder)));
        self.audio_decoder = Some(Arc::new(Mutex::new(audio_decoder)));
        self.format_input = Some(Arc::new(Mutex::new(format_input)));
    }
    pub fn change_file_path_and_init_par(&mut self, file_path: &Path) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let format_input = format::input(file_path).unwrap();
        let video_stream = format_input.streams().best(Type::Video).unwrap();
        let audio_stream = format_input.streams().best(Type::Audio).unwrap();
        self.audio_start_time = audio_stream.start_time();
        self.video_start_time = video_stream.start_time();
        {
            let mut mutex_guard = self.video_stream_index.lock().unwrap();
            *mutex_guard = video_stream.index();
        }
        {
            let mut mutex_guard = self.audio_stream_index.lock().unwrap();
            *mutex_guard = audio_stream.index();
        }
        let v_frames = video_stream.frames();
        let fps = video_stream.avg_frame_rate().0;
        self.video_time_base = video_stream.time_base().1;
        warn!("video time_base==={}", self.video_time_base);
        self.video_frame_rate = fps;
        warn!("video_frame_rate==={}", self.video_frame_rate);
        self.audio_time_base = audio_stream.time_base().1;
        warn!("audio time_base==={}", self.audio_time_base);
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
        self.end_video_ts = v_frames / fps as i64 * self.video_time_base as i64;
        let video_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(video_stream.parameters()).unwrap();
        let audio_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(audio_stream.parameters()).unwrap();
        let video_decoder = video_decoder_ctx.decoder().video().unwrap();
        warn!(
            "video decoder width=={} height=={}",
            video_decoder.width(),
            video_decoder.height()
        );

        self.converter_ctx = Some(
            ffmpeg_the_third::software::converter(
                (video_decoder.width(), video_decoder.height()),
                video_decoder.format(),
                Pixel::RGBA,
            )
            .unwrap(),
        );
        self.video_frame_rect = [video_decoder.width(), video_decoder.height()];
        let audio_decoder = audio_decoder_ctx.decoder().audio().unwrap();
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
                    audio_decoder.rate(),
                ),
            )
            .unwrap(),
        );
        self.input_play_end_flag
            .store(false, std::sync::atomic::Ordering::Release);
        {
            let mut mutex_guard = self.packet_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.video_frame_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.audio_frame_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.video_decoder.as_ref().unwrap().lock().unwrap();
            *mutex_guard = video_decoder;
            let mut mutex_guard = self.audio_decoder.as_ref().unwrap().lock().unwrap();
            *mutex_guard = audio_decoder;
            let mut mutex_guard = self.format_input.as_ref().unwrap().lock().unwrap();
            *mutex_guard = format_input;
        }
    }
    fn packet_demux_process(
        packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        format_input: Arc<Mutex<ffmpeg_the_third::format::context::Input>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        input_end_flag: Arc<AtomicBool>,
    ) {
        loop {
            if packet_demux_thread_stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let cache_len = {
                let rw_lock_guard = packet_cache_vec.read().unwrap();
                rw_lock_guard.len()
            };
            if cache_len >= 100 {
                sleep(Duration::from_millis(10));
                continue;
            }
            {
                /*
                这里注意chchevec和formatinput锁的先后顺序
                 */
                let mut rw_lock_guard = packet_cache_vec.write().unwrap();
                let mut lock_guard = format_input.lock().unwrap();
                match lock_guard.packets().next() {
                    Some(Ok((stream, packet))) => rw_lock_guard.push(packet),
                    Some(Err(e)) => {
                        if let ffmpeg_the_third::Error::Eof = e {
                            input_end_flag.store(true, std::sync::atomic::Ordering::Release);
                            warn!("set end flag !!!");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    fn frame_decode_process(
        video_stream_index: Arc<Mutex<usize>>,
        audio_stream_index: Arc<Mutex<usize>>,
        video_decoder: Arc<Mutex<ffmpeg_the_third::decoder::Video>>,
        audio_decoder: Arc<Mutex<ffmpeg_the_third::decoder::Audio>>,
        video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
        audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        loop {
            if frame_decode_thread_stop_flag.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            let video_frame_vec_len = {
                let guard = video_frame_cache_vec.read().unwrap();
                guard.len()
            };
            let audio_frame_vec_len = {
                let guard = audio_frame_cache_vec.read().unwrap();
                guard.len()
            };
            if video_frame_vec_len >= 100 && audio_frame_vec_len >= 100 {
                sleep(Duration::from_millis(10));
                continue;
            }

            {
                /*
                        注意这里packetcachevec的锁和decoer的锁同时拿到，
                        其余地方若需要同时使用需要用同样地顺序避免死锁,
                        同时拿到多锁是为了避免改变input时
                        遇到decoder和packet不匹配的情况
                */
                let mut rw_lock_write_guard = packet_cache_vec.write().unwrap();
                let front_packet;
                if rw_lock_write_guard.len() > 0 {
                    front_packet = rw_lock_write_guard.remove(0);
                } else {
                    continue;
                }
                if front_packet.stream() == *video_stream_index.lock().unwrap() {
                    {
                        let mut lock_guard = video_decoder.lock().unwrap();
                        lock_guard.send_packet(&front_packet).unwrap();

                        loop {
                            let mut video_frame_tmp = ffmpeg_the_third::frame::Video::empty();
                            if let Err(e) = lock_guard.receive_frame(&mut video_frame_tmp) {
                                break;
                            }
                            // warn!("decode video frame ts:{}",video_frame_tmp.timestamp().unwrap());
                            // warn!("decode video frame pts:{}",video_frame_tmp.pts().unwrap());
                            let mut rw_lock_write_guard1 = video_frame_cache_vec.write().unwrap();
                            rw_lock_write_guard1.push(video_frame_tmp);
                        }
                    }
                } else if front_packet.stream() == *audio_stream_index.lock().unwrap() {
                    {
                        let mut lock_guard = audio_decoder.lock().unwrap();
                        lock_guard.send_packet(&front_packet).unwrap();
                        loop {
                            let mut audio_frame_tmp = ffmpeg_the_third::frame::Audio::empty();
                            if let Err(e) = lock_guard.receive_frame(&mut audio_frame_tmp) {
                                break;
                            }
                            let mut rw_lock_write_guard1 = audio_frame_cache_vec.write().unwrap();
                            rw_lock_write_guard1.push(audio_frame_tmp);
                        }
                    }
                }
            }
        }
    }
    pub fn start_process_threads(&mut self) {
        let packet_demux_thread_stop_flag = self.packet_demux_thread_stop_flag.clone();
        let format_input = self.format_input.as_ref().unwrap().clone();
        let packet_cache_vec = self.packet_cache_vec.clone();
        let input_end_flag = self.input_play_end_flag.clone();
        self.packet_demux_thread_handler = Some(
            thread::Builder::new()
                .name("demux thread".to_string())
                .spawn(move || {
                    Self::packet_demux_process(
                        packet_demux_thread_stop_flag,
                        format_input,
                        packet_cache_vec,
                        input_end_flag,
                    );
                })
                .unwrap(),
        );

        let video_stream_index = self.video_stream_index.clone();
        let audio_stream_index = self.audio_stream_index.clone();
        let video_decoder = self.video_decoder.as_ref().unwrap().clone();
        let audio_decoder = self.audio_decoder.as_ref().unwrap().clone();
        let video_frame_cache_vec = self.video_frame_cache_vec.clone();
        let audio_frame_cache_vec = self.audio_frame_cache_vec.clone();
        let packet_cache_vec = self.packet_cache_vec.clone();
        let frame_decode_thread_stop_flag = self.frame_decode_thread_stop_flag.clone();
        self.frame_decode_thread_handler = Some(
            std::thread::Builder::new()
                .name("decode thread".to_string())
                .spawn(move || {
                    Self::frame_decode_process(
                        video_stream_index,
                        audio_stream_index,
                        video_decoder,
                        audio_decoder,
                        video_frame_cache_vec,
                        audio_frame_cache_vec,
                        packet_cache_vec,
                        frame_decode_thread_stop_flag,
                    );
                })
                .unwrap(),
        );
    }
    pub fn get_one_audio_play_frame_and_pts(
        &mut self,
    ) -> (Option<ffmpeg_the_third::frame::Audio>, i64) {
        let resampler_ctx = self.resampler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Audio::empty();
        let raw_frame;
        {
            let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                raw_frame = rw_lock_write_guard.remove(0);
            } else {
                return (None, 0);
            }
        }
        let pts = raw_frame.pts().unwrap();
        resampler_ctx.run(&raw_frame, &mut res).unwrap();

        return (Some(res), pts);
    }
    pub fn get_one_video_play_frame_and_pts(
        &mut self,
    ) -> (Option<ffmpeg_the_third::frame::Video>, i64) {
        let converter_ctx = self.converter_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Video::empty();
        let mut return_val = (None, 0);
        {
            let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                let raw_frame = rw_lock_write_guard.remove(0);
                let pts = raw_frame.pts().unwrap();
                converter_ctx.run(&raw_frame, &mut res).unwrap();
                return_val = (Some(res), pts);
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
    pub fn get_video_time_base(&self) -> i32 {
        return self.video_time_base;
    }
    pub fn get_audio_time_base(&self) -> i32 {
        return self.audio_time_base;
    }
    pub fn get_video_frame_rate(&self) -> i32 {
        return self.video_frame_rate;
    }
    pub fn get_total_video_time_formatted_string(&self) -> &String {
        return &self.total_video_time_formatted_string;
    }
    pub fn get_video_frame_rect(&self) -> &[u32; 2] {
        return &self.video_frame_rect;
    }
    pub fn get_video_start_time(&self) -> i64 {
        return self.video_start_time;
    }
    pub fn get_audio_start_time(&self) -> i64 {
        return self.audio_start_time;
    }
    pub fn get_end_video_ts(&self) -> i64 {
        return self.end_video_ts;
    }
    pub fn get_input_end_flag(&self) -> Arc<AtomicBool> {
        return self.input_play_end_flag.clone();
    }
    pub fn seek_timestamp_to_decode(&self, ts: i64) {
        {
            let mut rw_lock_write_guard = self.packet_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();

            let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();

            let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();
            let mut lock_guard = self.format_input.as_ref().unwrap().lock().unwrap();
            unsafe {
                let res = ffmpeg_the_third::ffi::avformat_seek_file(
                    lock_guard.as_mut_ptr(),
                    (*self.video_stream_index.lock().unwrap()) as i32,
                    ts - self.video_time_base as i64,
                    ts,
                    ts + self.video_time_base as i64,
                    ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY,
                );
                if res < 0 {
                    warn!("seek err num:{res}");
                }
            }
        }
    }
}
