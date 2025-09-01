use std::{
    path::Path,
    ptr::{null, null_mut},
    sync::{Arc, RwLock},
    thread::{self, sleep},
    time::Duration,
};

use ffmpeg_the_third::{
    Stream,
    codec::Id,
    ffi::{AVBufferRef, av_hwdevice_ctx_create, av_hwframe_transfer_data, avcodec_get_hw_config},
    format::{self, Pixel},
    media::Type,
};
use log::warn;
use time::format_description;
pub struct ManualProtectedInput(ffmpeg_the_third::format::context::Input);
unsafe impl Send for ManualProtectedInput {}
unsafe impl Sync for ManualProtectedInput {}
pub struct ManualProtectedVideoDecoder(ffmpeg_the_third::decoder::Video);
unsafe impl Send for ManualProtectedVideoDecoder {}
unsafe impl Sync for ManualProtectedVideoDecoder {}
pub struct ManualProtectedAudioDecoder(ffmpeg_the_third::decoder::Audio);
unsafe impl Send for ManualProtectedAudioDecoder {}
unsafe impl Sync for ManualProtectedAudioDecoder {}
pub struct TinyDecoder {
    video_stream_index: Arc<RwLock<usize>>,
    audio_stream_index: Arc<RwLock<usize>>,
    video_time_base: i32,
    audio_time_base: i32,
    video_frame_rect: [u32; 2],
    end_audio_ts: i64,
    end_time_formatted_string: String,
    format_input: Option<Arc<RwLock<ManualProtectedInput>>>,
    video_decoder: Option<Arc<RwLock<ManualProtectedVideoDecoder>>>,
    audio_decoder: Option<Arc<RwLock<ManualProtectedAudioDecoder>>>,
    converter_ctx: Option<ffmpeg_the_third::software::scaling::Context>,
    resampler_ctx: Option<ffmpeg_the_third::software::resampling::Context>,
    video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
    audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
    packet_demux_thread_handler: Option<thread::JoinHandle<()>>,
    packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
    frame_decode_thread_handler: Option<thread::JoinHandle<()>>,
    frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    hardware_config: Arc<RwLock<bool>>,
}
impl TinyDecoder {
    pub fn new() -> Self {
        ffmpeg_the_third::init().unwrap();
        return Self {
            video_stream_index: Arc::new(RwLock::new(0)),
            audio_stream_index: Arc::new(RwLock::new(0)),
            video_time_base: 0,
            audio_time_base: 0,
            video_frame_rect: [0, 0],
            end_audio_ts: 0,
            end_time_formatted_string: String::new(),
            format_input: None,
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
            hardware_config: Arc::new(RwLock::new(false)),
        };
    }

    pub fn set_file_path_and_init_par(&mut self, file_path: &Path) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let format_input = ffmpeg_the_third::format::input(file_path).unwrap();
        let video_stream = format_input.streams().best(Type::Video).unwrap();
        let audio_stream = format_input.streams().best(Type::Audio).unwrap();
        {
            let mut mutex_guard = self.video_stream_index.write().unwrap();
            *mutex_guard = video_stream.index();
        }
        {
            let mut mutex_guard = self.audio_stream_index.write().unwrap();
            *mutex_guard = audio_stream.index();
        }

        self.video_time_base = video_stream.time_base().1;
        warn!("video time_base==={}", self.video_time_base);
        self.audio_time_base = audio_stream.time_base().1;
        warn!("audio time_base==={}", self.audio_time_base);
        // format_input.duration() 能较准确得到视频文件的总时常，mkv等格式可能存在
        // stream不存储时长,format_input.duration()更可靠
        // format_input.duration() 单位是微妙
        let adur_ts = format_input.duration() * self.audio_time_base as i64 / 1000_000;
        self.end_audio_ts = adur_ts;
        warn!("audio end ts:{}", self.end_audio_ts);
        self.compute_and_set_end_time_str();
        let video_decode_ctx = self.choose_decoder_with_hardware_prefer(&video_stream);
        let audio_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(audio_stream.parameters()).unwrap();
        warn!(
            "video decoder width=={} height=={}",
            video_decode_ctx.width(),
            video_decode_ctx.height()
        );
        self.converter_ctx = Some(
            ffmpeg_the_third::software::converter(
                (video_decode_ctx.width(), video_decode_ctx.height()),
                video_decode_ctx.format(),
                Pixel::RGBA,
            )
            .unwrap(),
        );
        warn!("video decode format{:#?}", video_decode_ctx.format());
        self.video_frame_rect = [video_decode_ctx.width(), video_decode_ctx.height()];
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
        self.video_decoder = Some(Arc::new(RwLock::new(ManualProtectedVideoDecoder(
            video_decode_ctx,
        ))));
        self.audio_decoder = Some(Arc::new(RwLock::new(ManualProtectedAudioDecoder(
            audio_decoder,
        ))));
        self.format_input = Some(Arc::new(RwLock::new(ManualProtectedInput(format_input))));
    }
    pub fn change_file_path_and_init_par(&mut self, file_path: &Path) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let format_input = format::input(file_path).unwrap();
        let video_stream = format_input.streams().best(Type::Video).unwrap();
        let audio_stream = format_input.streams().best(Type::Audio).unwrap();
        {
            let mut mutex_guard = self.video_stream_index.write().unwrap();
            *mutex_guard = video_stream.index();
        }
        {
            let mut mutex_guard = self.audio_stream_index.write().unwrap();
            *mutex_guard = audio_stream.index();
        }

        let adur = audio_stream.duration();
        self.video_time_base = video_stream.time_base().1;
        warn!("video time_base==={}", self.video_time_base);
        self.audio_time_base = audio_stream.time_base().1;
        warn!("audio time_base==={}", self.audio_time_base);
        self.end_audio_ts = adur;
        self.compute_and_set_end_time_str();
        let video_decoder = self.choose_decoder_with_hardware_prefer(&video_stream);
        let audio_decoder_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(audio_stream.parameters()).unwrap();
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
        {
            let mut mutex_guard = self.packet_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.video_frame_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.audio_frame_cache_vec.write().unwrap();
            mutex_guard.clear();
            let mut mutex_guard = self.video_decoder.as_ref().unwrap().write().unwrap();
            *mutex_guard = ManualProtectedVideoDecoder(video_decoder);
            let mut mutex_guard = self.audio_decoder.as_ref().unwrap().write().unwrap();
            *mutex_guard = ManualProtectedAudioDecoder(audio_decoder);
            let mut mutex_guard = self.format_input.as_ref().unwrap().write().unwrap();
            *mutex_guard = ManualProtectedInput(format_input);
        }
    }
    fn packet_demux_process(
        packet_demux_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        format_input: Arc<RwLock<ManualProtectedInput>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
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
                let mut lock_guard = format_input.write().unwrap();
                match lock_guard.0.packets().next() {
                    Some(Ok((_stream, packet))) => rw_lock_guard.push(packet),
                    Some(Err(e)) => {
                        if let ffmpeg_the_third::util::error::Error::Eof = e {
                            warn!("demux process hit the end");
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    fn frame_decode_process(
        video_stream_index: Arc<RwLock<usize>>,
        audio_stream_index: Arc<RwLock<usize>>,
        video_decoder: Arc<RwLock<ManualProtectedVideoDecoder>>,
        audio_decoder: Arc<RwLock<ManualProtectedAudioDecoder>>,
        video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
        audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        frame_decode_thread_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
        hardware_config: Arc<RwLock<bool>>,
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
                if front_packet.stream() == *video_stream_index.read().unwrap() {
                    {
                        let mut lock_guard = video_decoder.write().unwrap();
                        lock_guard.0.send_packet(&front_packet).unwrap();

                        loop {
                            let mut video_frame_tmp = ffmpeg_the_third::frame::Video::empty();
                            if let Err(_e) = lock_guard.0.receive_frame(&mut video_frame_tmp) {
                                break;
                            }
                            // warn!("decode video frame ts:{}",video_frame_tmp.timestamp().unwrap());
                            // warn!("decode video frame pts:{}",video_frame_tmp.pts().unwrap());
                            let mut rw_lock_write_guard1 = video_frame_cache_vec.write().unwrap();
                            if !*hardware_config.read().unwrap() {
                                rw_lock_write_guard1.push(video_frame_tmp);
                            } else {
                                unsafe {
                                    let mut transfered_frame =
                                        ffmpeg_the_third::frame::Video::empty();
                                    if 0 != av_hwframe_transfer_data(
                                        transfered_frame.as_mut_ptr(),
                                        video_frame_tmp.as_ptr(),
                                        0,
                                    ) {
                                        warn!("hardware frame transfer to software frame err");
                                    }
                                    transfered_frame.set_pts(video_frame_tmp.pts());
                                    rw_lock_write_guard1.push(transfered_frame);
                                }
                            }
                        }
                    }
                } else if front_packet.stream() == *audio_stream_index.read().unwrap() {
                    {
                        let mut lock_guard = audio_decoder.write().unwrap();
                        lock_guard.0.send_packet(&front_packet).unwrap();
                        loop {
                            let mut audio_frame_tmp = ffmpeg_the_third::frame::Audio::empty();
                            if let Err(_e) = lock_guard.0.receive_frame(&mut audio_frame_tmp) {
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
        self.packet_demux_thread_handler = Some(
            thread::Builder::new()
                .name("demux thread".to_string())
                .spawn(move || {
                    Self::packet_demux_process(
                        packet_demux_thread_stop_flag,
                        format_input,
                        packet_cache_vec,
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
        let hardware_config = self.hardware_config.clone();
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
                        hardware_config,
                    );
                })
                .unwrap(),
        );
    }
    pub fn get_one_audio_play_frame_and_pts(&mut self) -> Option<ffmpeg_the_third::frame::Audio> {
        let resampler_ctx = self.resampler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Audio::empty();
        let raw_frame;
        {
            let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                raw_frame = rw_lock_write_guard.remove(0);
            } else {
                return None;
            }
        }

        resampler_ctx.run(&raw_frame, &mut res).unwrap();
        if let Some(pts) = raw_frame.pts() {
            res.set_pts(Some(pts));
        }
        return Some(res);
    }
    pub fn get_one_video_play_frame_and_pts(&mut self) -> Option<ffmpeg_the_third::frame::Video> {
        let converter_ctx = self.converter_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Video::empty();
        let mut return_val = None;
        {
            let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
            if rw_lock_write_guard.len() > 0 {
                let raw_frame = rw_lock_write_guard.remove(0);
                if raw_frame.format() != converter_ctx.input().format {
                    *converter_ctx = ffmpeg_the_third::software::converter(
                        (converter_ctx.input().width, converter_ctx.input().height),
                        raw_frame.format(),
                        Pixel::RGBA,
                    )
                    .unwrap();
                }
                converter_ctx.run(&raw_frame, &mut res).unwrap();
                if let Some(pts) = raw_frame.pts() {
                    res.set_pts(Some(pts));
                }
                return_val = Some(res);
            }
        }
        return return_val;
    }
    pub fn get_input_par(&self) -> Option<Arc<RwLock<ManualProtectedInput>>> {
        if self.format_input.is_none() {
            return None;
        }
        return Some(self.format_input.as_ref().unwrap().clone());
    }
    pub fn get_video_time_base(&self) -> i32 {
        return self.video_time_base;
    }
    pub fn get_audio_time_base(&self) -> i32 {
        return self.audio_time_base;
    }
    pub fn get_end_time_formatted_string(&self) -> &String {
        return &self.end_time_formatted_string;
    }
    pub fn get_video_frame_rect(&self) -> &[u32; 2] {
        return &self.video_frame_rect;
    }
    pub fn get_end_audio_ts(&self) -> i64 {
        return self.end_audio_ts;
    }
    pub fn seek_timestamp_to_decode(&self, ts: i64) {
        {
            if let Ok(mut input) = self.format_input.as_ref().unwrap().write() {
                unsafe {
                    if let Ok(a_stream_idx) = self.audio_stream_index.read() {
                        warn!("seek ts:{}", ts);
                        let res = ffmpeg_the_third::ffi::avformat_seek_file(
                            input.0.as_mut_ptr(),
                            *a_stream_idx as i32,
                            ts - self.audio_time_base as i64,
                            ts,
                            ts + self.audio_time_base as i64,
                            ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY,
                        );
                        if res != 0 {
                            warn!("seek err num:{res}");
                        }
                    }
                }
            }
            let mut rw_lock_write_guard = self.packet_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();

            let mut rw_lock_write_guard = self.video_frame_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();

            let mut rw_lock_write_guard = self.audio_frame_cache_vec.write().unwrap();
            rw_lock_write_guard.clear();
        }
    }

    fn compute_and_set_end_time_str(&mut self) {
        let sec_num = self.end_audio_ts / self.audio_time_base as i64;
        let sec = (sec_num % 60) as u8;
        let min_num = sec_num / 60;
        let min = (min_num % 60) as u8;
        let hour_num = min_num / 60;
        let hour = hour_num as u8;
        warn!("hour{},min{},sec{}", hour, min, sec);
        if let Ok(time) = time::Time::from_hms(hour, min, sec) {
            if let Ok(formatter) = format_description::parse("[hour]:[minute]:[second]") {
                if let Ok(s) = time.format(&formatter) {
                    self.end_time_formatted_string = s;
                }
            }
        } else {
            warn!("end_time_err");
        }
    }
}
impl TinyDecoder {
    fn choose_decoder_with_hardware_prefer(
        &mut self,
        stream: &Stream<'_>,
    ) -> ffmpeg_the_third::decoder::Video {
        let mut codec_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(stream.parameters()).unwrap();
        let codec_id = stream.parameters().id();
        let decoder: ffmpeg_the_third::decoder::Video;
        if codec_id.eq(&Id::H264) || codec_id.eq(&Id::H265) || codec_id.eq(&Id::HEVC) {
            unsafe {
                let mut hw_device_ctx: *mut AVBufferRef = null_mut();
                if 0 != av_hwdevice_ctx_create(
                    &mut hw_device_ctx as *mut *mut AVBufferRef,
                    ffmpeg_the_third::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D12VA,
                    null(),
                    null_mut(),
                    0,
                ) {
                    warn!("hw device create err");
                    return codec_ctx.decoder().video().unwrap();
                }
                (*codec_ctx.as_mut_ptr()).hw_device_ctx = hw_device_ctx;
                decoder = codec_ctx.decoder().video().unwrap();
                let hw_config = avcodec_get_hw_config(decoder.codec().unwrap().as_ptr(), 0);
                if hw_config.is_null() {
                    warn!("currently dont support hardware accelerate");
                    return decoder;
                }
                *self.hardware_config.write().unwrap() = true;
            }
        } else {
            decoder = codec_ctx.decoder().video().unwrap();
        }
        decoder
    }
}
