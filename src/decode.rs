use std::{
    ptr::{null, null_mut},
    sync::Arc,
    time::Duration,
    usize,
};

use ffmpeg_the_third::{
    Rational, Stream,
    codec::Id,
    ffi::{AVBufferRef, av_hwdevice_ctx_create, av_hwframe_transfer_data, avcodec_get_hw_config},
    format::{self, Pixel},
};
use log::warn;
use time::format_description;
use tokio::{runtime::Runtime, sync::RwLock, task::JoinHandle, time::sleep};

use crate::appui::VideoPathSource;
/// in order to make Input impl Sync, create a new Type ,
/// this type can be manually sync by Rwlock
pub struct ManualProtectedInput(ffmpeg_the_third::format::context::Input);

unsafe impl Sync for ManualProtectedInput {}
/// in order to make decoder Video impl Sync, create a new Type ,
/// this type can be manually sync by Rwlock
pub struct ManualProtectedVideoDecoder(ffmpeg_the_third::decoder::Video);

unsafe impl Sync for ManualProtectedVideoDecoder {}
/// in order to make Input impl Sync, create a new Type ,
/// this type can be manually sync by Rwlock
pub struct ManualProtectedAudioDecoder(ffmpeg_the_third::decoder::Audio);

unsafe impl Sync for ManualProtectedAudioDecoder {}

/// represent all the details and relevent variables about
/// video format, decode, detail and hardware accelerate
pub struct TinyDecoder {
    video_stream_index: Arc<RwLock<usize>>,
    audio_stream_index: Arc<RwLock<usize>>,
    cover_stream_index: Arc<RwLock<usize>>,
    video_time_base: Rational,
    audio_time_base: Rational,
    video_frame_rect: [u32; 2],
    format_duration: Arc<RwLock<i64>>,
    end_audio_ts: i64,
    end_time_formatted_string: String,
    format_input: Arc<RwLock<Option<ManualProtectedInput>>>,
    video_decoder: Arc<RwLock<Option<ManualProtectedVideoDecoder>>>,
    audio_decoder: Arc<RwLock<Option<ManualProtectedAudioDecoder>>>,
    converter_ctx: Option<ffmpeg_the_third::software::scaling::Context>,
    resampler_ctx: Option<ffmpeg_the_third::software::resampling::Context>,
    video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
    audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
    packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
    demux_exit_flag: Arc<RwLock<bool>>,
    decode_exit_flag: Arc<RwLock<bool>>,
    demux_task_handle: Option<JoinHandle<()>>,
    decode_task_handle: Option<JoinHandle<()>>,
    hardware_config: Arc<RwLock<bool>>,
    cover_pic_data: Arc<RwLock<Option<Vec<u8>>>>,
    async_rt: Arc<Runtime>,
}
impl TinyDecoder {
    /// init Decoder and new Struct
    pub fn new(runtime: Arc<Runtime>) -> Self {
        ffmpeg_the_third::init().unwrap();
        return Self {
            video_stream_index: Arc::new(RwLock::new(0)),
            audio_stream_index: Arc::new(RwLock::new(0)),
            cover_stream_index: Arc::new(RwLock::new(usize::MAX)),
            video_time_base: Rational::new(1, 1),
            audio_time_base: Rational::new(1, 1),
            video_frame_rect: [0, 0],
            format_duration: Arc::new(RwLock::new(0)),
            end_audio_ts: 0,
            end_time_formatted_string: String::new(),
            format_input: Arc::new(RwLock::new(None)),
            video_decoder: Arc::new(RwLock::new(None)),
            audio_decoder: Arc::new(RwLock::new(None)),
            converter_ctx: None,
            resampler_ctx: None,
            video_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            audio_frame_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            packet_cache_vec: std::sync::Arc::new(RwLock::new(vec![])),
            demux_exit_flag: Arc::new(RwLock::new(false)),
            decode_exit_flag: Arc::new(RwLock::new(false)),
            demux_task_handle: None,
            decode_task_handle: None,
            hardware_config: Arc::new(RwLock::new(false)),
            cover_pic_data: Arc::new(RwLock::new(None)),
            async_rt: runtime,
        };
    }
    /// called when user selected a file path to play
    /// init all the details from the file selected
    pub async fn set_file_path_and_init_par(&mut self, path: VideoPathSource) {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        let format_input = {
            if let VideoPathSource::TcpStream(s) = &path {
                ffmpeg_the_third::format::input(s).unwrap()
            } else if let VideoPathSource::File(s) = &path {
                ffmpeg_the_third::format::input(s).unwrap()
            } else {
                todo!();
            }
        };
        warn!("input construct finished");
        let mut cover_stream = None;
        let mut video_stream = None;
        let mut audio_stream = None;

        format_input.streams().for_each(|item| {
            let stream_type = item.parameters().medium();
            if stream_type == ffmpeg_the_third::util::media::Type::Video {
                warn!("找到视频流");
                video_stream = Some(item);
            } else if stream_type == ffmpeg_the_third::util::media::Type::Audio {
                warn!("找到音频流");
                audio_stream = Some(item);
            } else if stream_type == ffmpeg_the_third::util::media::Type::Attachment {
                warn!("找到attachment流");
                cover_stream = Some(item);
            } else {
                warn!("unhandled stream");
            }
        });
        if let Some(stream) = &cover_stream {
            warn!("cover stream found");
            let mut mutex_guard = self.cover_stream_index.write().await;
            *mutex_guard = stream.index();
        }
        if let Some(stream) = &video_stream {
            let mut mutex_guard = self.video_stream_index.write().await;
            *mutex_guard = stream.index();
            self.video_time_base = stream.time_base();
            warn!("video time_base==={}", self.video_time_base);
        }
        if let Some(stream) = &audio_stream {
            let mut mutex_guard = self.audio_stream_index.write().await;
            *mutex_guard = stream.index();
            self.audio_time_base = stream.time_base();
            warn!("audio time_base==={}", self.audio_time_base);
        }

        // format_input.duration() 能较准确得到视频文件的总时常，mkv等格式可能存在
        // stream不存储时长,format_input.duration()更可靠
        // format_input.duration() 单位是微妙
        if let VideoPathSource::File(_s) = &path {
            warn!("dur {} us", format_input.duration());
            *self.format_duration.write().await = format_input.duration();
            let adur_ts = format_input.duration() * self.audio_time_base.denominator() as i64
                / self.audio_time_base.numerator() as i64
                / 1000_000;
            self.end_audio_ts = adur_ts;
            warn!("audio end ts:{}", adur_ts);
            self.compute_and_set_end_time_str(adur_ts);
        } else {
            self.end_audio_ts = 0;
        }
        let video_decoder = self
            .choose_decoder_with_hardware_prefer(video_stream.as_ref().unwrap())
            .await;
        let audio_decoder_ctx = ffmpeg_the_third::codec::Context::from_parameters(
            audio_stream.as_ref().unwrap().parameters(),
        )
        .unwrap();

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
        warn!("video decode format{:#?}", video_decoder.format());
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
                    48000,
                ),
            )
            .unwrap(),
        );
        {
            let mut packet_cache_vec = self.packet_cache_vec.write().await;
            let mut audio_cache_vec = self.audio_frame_cache_vec.write().await;
            let mut video_cache_vec = self.video_frame_cache_vec.write().await;
            packet_cache_vec.clear();
            audio_cache_vec.clear();
            video_cache_vec.clear();
            //     self.clear_all_cache_vec().await;
            let mut v_decoder = self.video_decoder.write().await;
            *v_decoder = Some(ManualProtectedVideoDecoder(video_decoder));
            let mut a_decoder = self.audio_decoder.write().await;
            *a_decoder = Some(ManualProtectedAudioDecoder(audio_decoder));
            warn!("assign input");
            let mut input = self.format_input.write().await;
            *input = Some(ManualProtectedInput(format_input));
        }
        warn!("par init finished!!!");
    }
    /// the loop of demux video file
    async fn packet_demux_process(
        exit_flag: Arc<RwLock<bool>>,
        format_input: Arc<RwLock<Option<ManualProtectedInput>>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        cover_stream_index: Arc<RwLock<usize>>,
        cover_pic_data: Arc<RwLock<Option<Vec<u8>>>>,
    ) {
        loop {
            if *exit_flag.read().await {
                break;
            }
            /*
            I choose to lock the packet vec first stick this in other functions
             */
            let cache_len = {
                let rw_lock_guard = packet_cache_vec.read().await;
                rw_lock_guard.len()
            };
            if cache_len >= 100 {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            let mut input_end_flag = false;
            {
                let mut rw_lock_guard = packet_cache_vec.write().await;
                let cover_index = cover_stream_index.read().await;
                let mut cover_pic_data = cover_pic_data.write().await;
                let mut input = format_input.write().await;

                match input.as_mut().unwrap().0.packets().next() {
                    Some(Ok((_stream, packet))) => {
                        if packet.stream() == *cover_index {
                            *cover_pic_data = Some(packet.data().unwrap().to_vec());
                        } else {
                            rw_lock_guard.push(packet);
                        }
                    }
                    Some(Err(e)) => {
                        if let ffmpeg_the_third::util::error::Error::Eof = e {
                            warn!("demux process hit the end");
                        }
                    }
                    None => {
                        input_end_flag = true;
                    }
                }
            }
            if input_end_flag {
                sleep(Duration::from_millis(10)).await;
            }
        }
    }
    /// the loop of decode demuxed packet     
    async fn frame_decode_process(
        exit_flag: Arc<RwLock<bool>>,
        video_stream_index: Arc<RwLock<usize>>,
        audio_stream_index: Arc<RwLock<usize>>,
        video_decoder: Arc<RwLock<Option<ManualProtectedVideoDecoder>>>,
        audio_decoder: Arc<RwLock<Option<ManualProtectedAudioDecoder>>>,
        video_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Video>>>,
        audio_frame_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::frame::Audio>>>,
        packet_cache_vec: std::sync::Arc<RwLock<Vec<ffmpeg_the_third::packet::Packet>>>,
        hardware_config: Arc<RwLock<bool>>,
    ) {
        loop {
            if *exit_flag.read().await {
                break;
            }
            let video_frame_vec_len = {
                let guard = video_frame_cache_vec.read().await;
                guard.len()
            };
            let audio_frame_vec_len = {
                let guard = audio_frame_cache_vec.read().await;
                guard.len()
            };
            if video_frame_vec_len >= 100 && audio_frame_vec_len >= 100 {
                sleep(Duration::from_millis(10)).await;
                continue;
            }

            {
                /*
                        注意这里packetcachevec的锁和decoer的锁同时拿到，
                        其余地方若需要同时使用需要用同样地顺序避免死锁,
                        同时拿到多锁是为了避免改变input时
                        遇到decoder和packet不匹配的情况
                */
                let packet_cache_len = {
                    let packet_cache_vec = packet_cache_vec.write().await;
                    packet_cache_vec.len()
                };
                if packet_cache_len <= 0 {
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
                let mut rw_lock_write_guard = packet_cache_vec.write().await;
                let front_packet = rw_lock_write_guard.remove(0);

                if front_packet.stream() == *video_stream_index.read().await {
                    {
                        let mut v_frame_vec = video_frame_cache_vec.write().await;
                        let mut v_decoder = video_decoder.write().await;
                        v_decoder
                            .as_mut()
                            .unwrap()
                            .0
                            .send_packet(&front_packet)
                            .unwrap();

                        loop {
                            let mut video_frame_tmp = ffmpeg_the_third::frame::Video::empty();
                            if let Err(_e) = v_decoder
                                .as_mut()
                                .unwrap()
                                .0
                                .receive_frame(&mut video_frame_tmp)
                            {
                                break;
                            }
                            if !*hardware_config.read().await {
                                v_frame_vec.push(video_frame_tmp);
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
                                    v_frame_vec.push(transfered_frame);
                                }
                            }
                        }
                    }
                } else if front_packet.stream() == *audio_stream_index.read().await {
                    {
                        let mut a_frame_vec = audio_frame_cache_vec.write().await;
                        let mut audio_decoder = audio_decoder.write().await;
                        audio_decoder
                            .as_mut()
                            .unwrap()
                            .0
                            .send_packet(&front_packet)
                            .unwrap();
                        loop {
                            let mut audio_frame_tmp = ffmpeg_the_third::frame::Audio::empty();
                            if let Err(_e) = audio_decoder
                                .as_mut()
                                .unwrap()
                                .0
                                .receive_frame(&mut audio_frame_tmp)
                            {
                                break;
                            }

                            a_frame_vec.push(audio_frame_tmp);
                        }
                    }
                }
            }
        }
    }
    /// start the demux and decode task,pass in tokio runtime
    pub async fn start_process_input(&mut self) {
        let format_input = self.format_input.clone();
        let packet_cache_vec = self.packet_cache_vec.clone();
        let cov_stream_index = self.cover_stream_index.clone();
        let cover_pic_data = self.cover_pic_data.clone();
        let demux_exit_flag = self.demux_exit_flag.clone();
        self.demux_task_handle = Some(tokio::spawn(async move {
            Self::packet_demux_process(
                demux_exit_flag,
                format_input,
                packet_cache_vec,
                cov_stream_index,
                cover_pic_data,
            )
            .await;
        }));

        let video_stream_index = self.video_stream_index.clone();
        let audio_stream_index = self.audio_stream_index.clone();
        let video_decoder = self.video_decoder.clone();
        let audio_decoder = self.audio_decoder.clone();
        let video_frame_cache_vec = self.video_frame_cache_vec.clone();
        let audio_frame_cache_vec = self.audio_frame_cache_vec.clone();
        let packet_cache_vec = self.packet_cache_vec.clone();
        let hardware_config = self.hardware_config.clone();
        let decode_exit_flag = self.decode_exit_flag.clone();
        self.decode_task_handle = Some(tokio::spawn(async move {
            Self::frame_decode_process(
                decode_exit_flag,
                video_stream_index,
                audio_stream_index,
                video_decoder,
                audio_decoder,
                video_frame_cache_vec,
                audio_frame_cache_vec,
                packet_cache_vec,
                hardware_config,
            )
            .await;
        }));
    }
    /// called by the main thread pull one audio frame from the vec
    /// in addition, do the resample     
    pub async fn get_one_audio_play_frame_and_pts(
        &mut self,
    ) -> Option<ffmpeg_the_third::frame::Audio> {
        let resampler_ctx = self.resampler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Audio::empty();
        let raw_frame;
        {
            let mut a_frame_vec = self.audio_frame_cache_vec.write().await;
            if a_frame_vec.len() > 0 {
                raw_frame = a_frame_vec.remove(0);
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
    /// pull one frame from the video cache vec
    /// in additon, do the convert and if the input changed(caused by source or hard acce)
    /// set the new converter, only change the out put format, dont change the width and height which
    /// have been used in the ui thread    
    pub async fn pull_one_video_play_frame(&mut self) -> Option<ffmpeg_the_third::frame::Video> {
        let converter_ctx = self.converter_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Video::empty();
        let mut return_val = None;
        {
            let mut v_frame_vec = self.video_frame_cache_vec.write().await;
            if v_frame_vec.len() > 0 {
                let raw_frame = v_frame_vec.remove(0);
                if raw_frame.format() != converter_ctx.input().format {
                    *converter_ctx = ffmpeg_the_third::software::converter(
                        (raw_frame.width(), raw_frame.height()),
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
        return_val
    }
    /// get v time base used to check time and compare to sync     
    pub fn get_video_time_base(&self) -> &Rational {
        &self.video_time_base
    }
    /// get a time base used to check time and compare to sync    
    pub fn get_audio_time_base(&self) -> &Rational {
        &self.audio_time_base
    }
    /// get the calculated end time str     
    pub fn get_end_time_formatted_string(&self) -> &String {
        return &self.end_time_formatted_string;
    }
    /// get_video_frame_rect to config the main colorimage and texture size
    pub fn get_video_frame_rect(&self) -> &[u32; 2] {
        &self.video_frame_rect
    }
    /// get the end audio timestamp used as the main time flow
    /// it is more accurate than just use time second     
    pub async fn get_end_audio_ts(&mut self) -> i64 {
        if self.end_audio_ts == 0 {
            let adur_ts = {
                let dur = self.format_duration.read().await;
                let adur_ts = *dur * self.audio_time_base.denominator() as i64
                    / self.audio_time_base.numerator() as i64
                    / 1000_000;
                self.end_audio_ts = adur_ts;
                warn!("audio end ts:{}", adur_ts);
                adur_ts
            };
            self.compute_and_set_end_time_str(adur_ts);
        }
        self.end_audio_ts
    }
    /// seek the input to a selected timestamp
    /// use the ffi function to enable seek all the frames
    /// the ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY flag makes sure
    /// the seek would go as I want, to an exact frame
    pub async fn seek_timestamp_to_decode(&mut self, ts: i64) {
        {
            let mut packet_cache_vec = self.packet_cache_vec.write().await;
            let mut audio_cache_vec = self.audio_frame_cache_vec.write().await;
            let mut video_cache_vec = self.video_frame_cache_vec.write().await;
            packet_cache_vec.clear();
            audio_cache_vec.clear();
            video_cache_vec.clear();
            let mut input = self.format_input.write().await;
            unsafe {
                let a_stream_idx = self.audio_stream_index.read().await;
                warn!("seek audio timestamp:{}", ts);
                let res = ffmpeg_the_third::ffi::avformat_seek_file(
                    input.as_mut().unwrap().0.as_mut_ptr(),
                    *a_stream_idx as i32,
                    ts - self.audio_time_base.denominator() as i64,
                    ts,
                    ts + self.audio_time_base.denominator() as i64,
                    ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY,
                );
                if res != 0 {
                    warn!("seek err num:{res}");
                }
            }
        }
    }
    /// use the file detail to compute the video duration and make str to inform the user
    fn compute_and_set_end_time_str(&mut self, end_ts: i64) {
        let sec_num = end_ts * self.audio_time_base.numerator() as i64
            / self.audio_time_base.denominator() as i64;
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
    pub fn get_cover_pic_data(&self) -> Arc<RwLock<Option<Vec<u8>>>> {
        self.cover_pic_data.clone()
    }
    pub async fn check_input_exist(&self) -> bool {
        let input = self.format_input.write().await;
        input.is_some()
    }
    pub fn get_format_duration(&self) -> Arc<RwLock<i64>> {
        self.format_duration.clone()
    }
}

impl TinyDecoder {
    /// enable hardware accelerate for video decode, currently use d3d12 only on windows
    /// others like vulkan are in developing
    /// fallback to softerware decoder if doesnt support
    async fn choose_decoder_with_hardware_prefer(
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
                    ffmpeg_the_third::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN,
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
                *self.hardware_config.write().await = true;
            }
        } else {
            decoder = codec_ctx.decoder().video().unwrap();
        }
        decoder
    }
}
impl Drop for TinyDecoder {
    fn drop(&mut self) {
        self.async_rt.block_on(async {
            *self.demux_exit_flag.write().await = true;
            self.demux_task_handle.as_mut().unwrap().await.unwrap();
            *self.decode_exit_flag.write().await = true;
            self.decode_task_handle.as_mut().unwrap().await.unwrap();
            warn!("demux and decode task exit gracefully!!!");
            self.packet_cache_vec.write().await.clear();
            self.audio_frame_cache_vec.write().await.clear();
            self.video_frame_cache_vec.write().await.clear();
            warn!("cache vec cleared gracefully!!!!");
        });
    }
}
