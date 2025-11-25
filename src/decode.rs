use std::{
    fmt::Error,
    ptr::{null, null_mut},
    sync::Arc,
    time::Duration,
    usize,
};

use ffmpeg_the_third::{
    Rational, Stream,
    ffi::{
        AVBufferRef, AVPixelFormat, av_hwdevice_ctx_create, av_hwframe_transfer_data,
        avcodec_get_hw_config,
    },
    filter::Graph,
    format::{self, Pixel},
    frame::Video,
};
use log::warn;
use time::format_description;
use tokio::{io::AsyncWriteExt, runtime::Runtime, sync::RwLock, task::JoinHandle, time::sleep};

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

pub enum MainStream {
    Video,
    Audio,
}
/// represent all the details and relevent variables about
/// video format, decode, detail and hardware accelerate
pub struct TinyDecoder {
    video_stream_index: Arc<RwLock<usize>>,
    audio_stream_index: Arc<RwLock<usize>>,
    cover_stream_index: Arc<RwLock<usize>>,
    main_stream: MainStream,
    video_time_base: Rational,
    audio_time_base: Rational,
    video_frame_rect: [u32; 2],
    format_duration: Arc<RwLock<i64>>,
    end_timestamp: i64,
    end_time_formatted_string: String,
    format_input: Arc<RwLock<Option<ManualProtectedInput>>>,
    video_decoder: Arc<RwLock<Option<ManualProtectedVideoDecoder>>>,
    audio_decoder: Arc<RwLock<Option<ManualProtectedAudioDecoder>>>,
    converter_ctx: Option<ffmpeg_the_third::software::scaling::Context>,
    resampler_ctx: Option<ffmpeg_the_third::software::resampling::Context>,
    video_frame_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::frame::Video>>>>,
    audio_frame_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::frame::Audio>>>>,
    audio_packet_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>>,
    video_packet_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>>,
    demux_exit_flag: Arc<RwLock<bool>>,
    decode_exit_flag: Arc<RwLock<bool>>,
    demux_task_handle: Option<JoinHandle<()>>,
    decode_task_handle: Option<JoinHandle<()>>,
    hardware_config: Arc<RwLock<bool>>,
    cover_pic_data: Arc<RwLock<Option<Vec<u8>>>>,
    async_rt: Arc<Runtime>,
    graph: Arc<RwLock<Option<ffmpeg_the_third::filter::Graph>>>,
}
impl TinyDecoder {
    /// init Decoder and new Struct
    pub fn new(runtime: Arc<Runtime>) -> Self {
        ffmpeg_the_third::init().unwrap();
        return Self {
            video_stream_index: Arc::new(RwLock::new(usize::MAX)),
            audio_stream_index: Arc::new(RwLock::new(usize::MAX)),
            cover_stream_index: Arc::new(RwLock::new(usize::MAX)),
            main_stream: MainStream::Audio,
            video_time_base: Rational::new(1, 1),
            audio_time_base: Rational::new(1, 1),
            video_frame_rect: [0, 0],
            format_duration: Arc::new(RwLock::new(0)),
            end_timestamp: 0,
            end_time_formatted_string: String::new(),
            format_input: Arc::new(RwLock::new(None)),
            video_decoder: Arc::new(RwLock::new(None)),
            audio_decoder: Arc::new(RwLock::new(None)),
            converter_ctx: None,
            resampler_ctx: None,
            video_frame_cache_vec: std::sync::Arc::new(RwLock::new(None)),
            audio_frame_cache_vec: std::sync::Arc::new(RwLock::new(None)),
            audio_packet_cache_vec: std::sync::Arc::new(RwLock::new(None)),
            video_packet_cache_vec: std::sync::Arc::new(RwLock::new(None)),
            demux_exit_flag: Arc::new(RwLock::new(false)),
            decode_exit_flag: Arc::new(RwLock::new(false)),
            demux_task_handle: None,
            decode_task_handle: None,
            hardware_config: Arc::new(RwLock::new(false)),
            cover_pic_data: Arc::new(RwLock::new(None)),
            async_rt: runtime,
            graph: Arc::new(RwLock::new(None)),
        };
    }
    async fn reset_tiny_decoder_states(&mut self) {
        *self.audio_stream_index.write().await = usize::MAX;
        *self.video_stream_index.write().await = usize::MAX;
        *self.cover_stream_index.write().await = usize::MAX;
        self.main_stream = MainStream::Audio;
        *self.audio_decoder.write().await = None;
        self.audio_time_base = Rational::new(1, 1);
        self.converter_ctx = None;
        *self.cover_pic_data.write().await = None;
        self.decode_task_handle = None;
        *self.decode_exit_flag.write().await = false;
        self.demux_task_handle = None;
        *self.demux_exit_flag.write().await = false;
        self.end_time_formatted_string = String::new();
        self.end_timestamp = 0;
        *self.format_duration.write().await = 0;
        *self.format_input.write().await = None;
        *self.graph.write().await = None;
        *self.hardware_config.write().await = false;
        self.resampler_ctx = None;
        *self.video_decoder.write().await = None;
        self.video_frame_rect = [0, 0];
        self.video_time_base = Rational::new(1, 1);
        *self.audio_packet_cache_vec.write().await = None;
        *self.video_packet_cache_vec.write().await = None;
        *self.audio_frame_cache_vec.write().await = None;
        *self.video_frame_cache_vec.write().await = None;
    }
    /// called when user selected a file path to play
    /// init all the details from the file selected
    pub async fn set_file_path_and_init_par(&mut self, path: VideoPathSource) -> Result<(), Error> {
        warn!("ffmpeg version{}", ffmpeg_the_third::format::version());
        if self.demux_task_handle.is_some() {
            self.stop_demux_and_decode().await;
            self.reset_tiny_decoder_states().await;
        }
        let format_input = {
            if let VideoPathSource::TcpStream(s) = &path {
                ffmpeg_the_third::format::input(s).unwrap()
            } else if let VideoPathSource::File(s) = &path {
                // let test_path = "http://127.0.0.1:23552/videos/泪桥/泪桥.m3u8";
                ffmpeg_the_third::format::input(s).unwrap()
            } else {
                todo!();
            }
        };
        warn!("input construct finished");
        let mut cover_stream = None;
        let mut video_stream = None;
        let mut audio_stream = None;

        for item in format_input.streams() {
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
        }
        if audio_stream.is_none() && video_stream.is_none() {
            panic!("no valid stream found");
        }
        if let Some(stream) = &cover_stream {
            warn!("cover stream found");
            let mut mutex_guard = self.cover_stream_index.write().await;
            *mutex_guard = stream.index();
        }

        if let Some(stream) = &audio_stream {
            let mut mutex_guard = self.audio_stream_index.write().await;
            *mutex_guard = stream.index();
            self.audio_time_base = stream.time_base();
            warn!("audio time_base==={}", self.audio_time_base);
            *self.audio_packet_cache_vec.write().await = Some(vec![]);
            *self.audio_frame_cache_vec.write().await = Some(vec![]);
        }

        if let Some(stream) = &video_stream {
            let mut mutex_guard = self.video_stream_index.write().await;
            *mutex_guard = stream.index();
            self.video_time_base = stream.time_base();
            warn!("video time_base==={}", self.video_time_base);
            if audio_stream.is_none() {
                self.main_stream = MainStream::Video;
            }
            *self.video_packet_cache_vec.write().await = Some(vec![]);
            *self.video_frame_cache_vec.write().await = Some(vec![]);
        }

        // format_input.duration() 能较准确得到视频文件的总时常，mkv等格式可能存在
        // stream不存储时长,format_input.duration()更可靠
        // format_input.duration() 单位是微妙
        if let VideoPathSource::File(_s) = &path {
            warn!("dur {} us", format_input.duration());
            *self.format_duration.write().await = format_input.duration();
            let adur_ts = {
                if let MainStream::Audio = self.main_stream {
                    format_input.duration() * self.audio_time_base.denominator() as i64
                        / self.audio_time_base.numerator() as i64
                        / 1000_000
                } else {
                    format_input.duration() * self.video_time_base.denominator() as i64
                        / self.video_time_base.numerator() as i64
                        / 1000_000
                }
            };
            self.end_timestamp = adur_ts;
            self.compute_and_set_end_time_str(adur_ts);
        }
        if video_stream.is_some() {
            let video_decoder = self
                .choose_decoder_with_hardware_prefer(video_stream.as_ref().unwrap())
                .await;
            self.converter_ctx = Some(
                ffmpeg_the_third::software::converter(
                    (video_decoder.width(), video_decoder.height()),
                    Pixel::YUV420P,
                    Pixel::RGBA,
                )
                .unwrap(),
            );
            warn!("video decode format{:#?}", video_decoder.format());
            self.video_frame_rect = [video_decoder.width(), video_decoder.height()];
            let mut v_decoder = self.video_decoder.write().await;
            *v_decoder = Some(ManualProtectedVideoDecoder(video_decoder));
        }
        if audio_stream.is_some() {
            let audio_decoder_ctx = ffmpeg_the_third::codec::Context::from_parameters(
                audio_stream.as_ref().unwrap().parameters(),
            )
            .unwrap();
            let audio_decoder = audio_decoder_ctx.decoder().audio().unwrap();
            self.resampler_ctx = Some(
                ffmpeg_the_third::software::resampler2(
                    (
                        audio_decoder.format(),
                        audio_decoder.ch_layout(),
                        audio_decoder.rate(),
                    ),
                    (
                        format::Sample::F32(format::sample::Type::Packed),
                        audio_decoder.ch_layout(),
                        48000,
                    ),
                )
                .unwrap(),
            );
            let mut a_decoder = self.audio_decoder.write().await;
            *a_decoder = Some(ManualProtectedAudioDecoder(audio_decoder));
        }
        {
            let graph = ffmpeg_the_third::filter::Graph::new();
            *self.graph.write().await = Some(graph);

            let mut input = self.format_input.write().await;
            *input = Some(ManualProtectedInput(format_input));
        }
        warn!("par init finished!!!");
        self.start_process_input().await;
        Ok(())
    }
    /// the loop of demux video file
    async fn packet_demux_process(
        audio_stream_index: Arc<RwLock<usize>>,
        video_stream_index: Arc<RwLock<usize>>,
        exit_flag: Arc<RwLock<bool>>,
        format_input: Arc<RwLock<Option<ManualProtectedInput>>>,
        audio_packet_cache_vec: std::sync::Arc<
            RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>,
        >,
        video_packet_cache_vec: std::sync::Arc<
            RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>,
        >,
        cover_stream_index: Arc<RwLock<usize>>,
        cover_pic_data: Arc<RwLock<Option<Vec<u8>>>>,
    ) {
        loop {
            sleep(Duration::from_millis(1)).await;
            if *exit_flag.read().await {
                break;
            }
            /*
            I choose to lock the packet vec first stick this in other functions
             */
            {
                let audio_packet_cache_vec = audio_packet_cache_vec.read().await;
                let video_packet_cache_vec = video_packet_cache_vec.read().await;
                if audio_packet_cache_vec.is_some() && video_packet_cache_vec.is_some() {
                    let a_packets = audio_packet_cache_vec.as_ref().unwrap();
                    let v_packets = video_packet_cache_vec.as_ref().unwrap();
                    //     warn!(
                    //         "audio packet vec len{},video packet vec len{}",
                    //         a_packets.len(),
                    //         v_packets.len()
                    //     );
                    if a_packets.len() >= 200 && v_packets.len() >= 200 {
                        continue;
                    }
                } else if let Some(a_packets) = &*audio_packet_cache_vec {
                    if a_packets.len() >= 200 {
                        // warn!("packet vec len{}",cache_len);
                        continue;
                    }
                } else if let Some(v_packets) = &*video_packet_cache_vec {
                    if v_packets.len() >= 200 {
                        // warn!("packet vec len{}",cache_len);
                        continue;
                    }
                }
            }

            {
                let mut audio_packet_vec = audio_packet_cache_vec.write().await;
                let mut video_packet_vec = video_packet_cache_vec.write().await;
                let audio_stream_idx = audio_stream_index.read().await;
                let video_stream_idx = video_stream_index.read().await;
                let cover_index = cover_stream_index.read().await;
                let mut cover_pic_data = cover_pic_data.write().await;
                let mut input = format_input.write().await;

                match input.as_mut().unwrap().0.packets().next() {
                    Some(Ok((stream, packet))) => {
                        if stream.index() == *cover_index {
                            *cover_pic_data = Some(packet.data().unwrap().to_vec());
                        } else if stream.index() == *audio_stream_idx {
                            audio_packet_vec.as_mut().unwrap().push(packet);
                        } else if stream.index() == *video_stream_idx {
                            video_packet_vec.as_mut().unwrap().push(packet);
                        }
                    }
                    Some(Err(e)) => {
                        if let ffmpeg_the_third::util::error::Error::Eof = e {
                            warn!("demux process hit the end");
                        }
                    }
                    None => {
                        warn!("input is end eof");
                    }
                }
            }
        }
    }
    /// the loop of decode demuxed packet
    async fn frame_decode_process(
        exit_flag: Arc<RwLock<bool>>,
        audio_decoder: Arc<RwLock<Option<ManualProtectedAudioDecoder>>>,
        video_decoder: Arc<RwLock<Option<ManualProtectedVideoDecoder>>>,
        audio_frame_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::frame::Audio>>>>,
        video_frame_cache_vec: std::sync::Arc<RwLock<Option<Vec<ffmpeg_the_third::frame::Video>>>>,
        audio_packet_cache_vec: std::sync::Arc<
            RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>,
        >,
        video_packet_cache_vec: std::sync::Arc<
            RwLock<Option<Vec<ffmpeg_the_third::packet::Packet>>>,
        >,
        hardware_config: Arc<RwLock<bool>>,
        graph: Arc<RwLock<Option<Graph>>>,
        video_time_base: Rational,
        video_frame_rect: [u32; 2],
    ) {
        if video_packet_cache_vec.read().await.is_some() {
            let exe_path = std::env::current_exe().unwrap();
            let exe_folder = exe_path.parent().unwrap();
            let p = exe_folder.join("app_font.ttf");
            if let Err(_) = tokio::fs::File::open(&p).await {
                if let Ok(mut file) = tokio::fs::File::create_new(&p).await {
                    if let Ok(_) = file.write_all(crate::appui::MAPLE_FONT).await {}
                }
            }
            {
                let mut graph = graph.write().await;
                let graph = graph.as_mut().unwrap();
                let args = {
                    let video_decoder = video_decoder.read().await;
                    let _decoder = &video_decoder.as_ref().unwrap().0;
                    //     warn!(
                    //         "video pixformat name{:?}",
                    //         decoder.format().descriptor().unwrap().name()
                    //     );
                    let font_path_str = p.to_str().unwrap();
                    let mut font_path_str = font_path_str.replace("\\", "/");
                    font_path_str.insert_str(font_path_str.find(":").unwrap(), "\\");
                    let spec = format!(
                        "drawtext=text='Tiny Player':fontfile='{}':fontsize=26:fontcolor=white@0.5:x=w-text_w-10:y=10",
                        font_path_str
                    );
                    format!(
                        "buffer=video_size={}x{}:pix_fmt={}:time_base={}/{}[in];[in]{}[out];[out]buffersink",
                        video_frame_rect[0],
                        video_frame_rect[1],
                        AVPixelFormat::AV_PIX_FMT_YUV420P as i32,
                        video_time_base.numerator(),
                        video_time_base.denominator(),
                        spec
                    )
                };

                graph.parse(&args).unwrap();

                graph.validate().unwrap();
                warn!("graph validate success!dump:\n{}", graph.dump());
            }
        }
        loop {
            sleep(Duration::from_millis(1)).await;
            if *exit_flag.read().await {
                break;
            }

            /*
                    注意这里packetcachevec的锁和decoer的锁同时拿到，
                    其余地方若需要同时使用需要用同样地顺序避免死锁,
                    同时拿到多锁是为了避免改变input时
                    遇到decoder和packet不匹配的情况
            */
            if audio_packet_cache_vec.read().await.is_some() {
                let mut audio_packet_cache_vec = audio_packet_cache_vec.write().await;
                let packets = audio_packet_cache_vec.as_mut().unwrap();
                let mut a_frame_vec = audio_frame_cache_vec.write().await;
                let frames = a_frame_vec.as_mut().unwrap();
                let mut audio_decoder = audio_decoder.write().await;
                // warn!("audio frame vec len{}", frames.len());
                if packets.len() > 0 && frames.len() < 10 {
                    let front_packet = packets.remove(0);
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

                        frames.push(audio_frame_tmp);
                    }
                }
            }
            if video_packet_cache_vec.read().await.is_some() {
                let mut video_packet_cache_vec = video_packet_cache_vec.write().await;
                let packets = video_packet_cache_vec.as_mut().unwrap();
                let mut v_frame_vec = video_frame_cache_vec.write().await;
                let frames = v_frame_vec.as_mut().unwrap();
                let mut v_decoder = video_decoder.write().await;
                // warn!("video frame vec len{}", frames.len());
                if packets.len() > 0 && frames.len() < 10 {
                    let front_packet = packets.remove(0);

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

                        let video_frame_tmp = {
                            if *hardware_config.read().await {
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
                                    let mut default_frame = Video::empty();
                                    {
                                        let mut hardware_frame_converter =
                                            ffmpeg_the_third::software::converter(
                                                (video_frame_tmp.width(), video_frame_tmp.height()),
                                                transfered_frame.format(),
                                                Pixel::YUV420P,
                                            )
                                            .unwrap();

                                        hardware_frame_converter
                                            .run(&transfered_frame, &mut default_frame)
                                            .unwrap();
                                        default_frame.set_pts(transfered_frame.pts());
                                        default_frame
                                    }
                                }
                            } else {
                                video_frame_tmp
                            }
                        };
                        let mut graph = graph.write().await;
                        let graph = graph.as_mut().unwrap();
                        if let Ok(_) = graph
                            .get("Parsed_buffer_0")
                            .as_mut()
                            .unwrap()
                            .source()
                            .add(&video_frame_tmp)
                        {
                            let mut filtered_frame = ffmpeg_the_third::frame::Video::empty();
                            if let Ok(_) = graph
                                .get("Parsed_buffersink_2")
                                .as_mut()
                                .unwrap()
                                .sink()
                                .frame(&mut filtered_frame)
                            {
                                frames.push(filtered_frame);
                            }
                        }
                    }
                }
            }
        }
    }

    /// start the demux and decode task,pass in tokio runtime
    pub async fn start_process_input(&mut self) {
        let audio_stream_index = self.audio_stream_index.clone();
        let video_stream_index = self.video_stream_index.clone();
        let format_input = self.format_input.clone();
        let audio_packet_cache_vec = self.audio_packet_cache_vec.clone();
        let video_packet_cache_vec = self.video_packet_cache_vec.clone();
        let cov_stream_index = self.cover_stream_index.clone();
        let cover_pic_data = self.cover_pic_data.clone();
        let demux_exit_flag = self.demux_exit_flag.clone();

        self.demux_task_handle = Some(tokio::spawn(async move {
            Self::packet_demux_process(
                audio_stream_index,
                video_stream_index,
                demux_exit_flag,
                format_input,
                audio_packet_cache_vec,
                video_packet_cache_vec,
                cov_stream_index,
                cover_pic_data,
            )
            .await;
        }));

        let video_decoder = self.video_decoder.clone();
        let audio_decoder = self.audio_decoder.clone();
        let video_frame_cache_vec = self.video_frame_cache_vec.clone();
        let audio_frame_cache_vec = self.audio_frame_cache_vec.clone();
        let audio_packet_cache_vec = self.audio_packet_cache_vec.clone();
        let video_packet_cache_vec = self.video_packet_cache_vec.clone();
        let hardware_config = self.hardware_config.clone();
        let decode_exit_flag = self.decode_exit_flag.clone();

        let graph = self.graph.clone();
        let video_time_base = self.video_time_base.clone();
        let video_frame_rect = self.video_frame_rect.clone();
        self.decode_task_handle = Some(tokio::spawn(async move {
            Self::frame_decode_process(
                decode_exit_flag,
                audio_decoder,
                video_decoder,
                audio_frame_cache_vec,
                video_frame_cache_vec,
                audio_packet_cache_vec,
                video_packet_cache_vec,
                hardware_config,
                graph,
                video_time_base,
                video_frame_rect,
            )
            .await;
        }));
    }
    /// called by the main thread pull one audio frame from the vec
    /// in addition, do the resample
    pub async fn pull_one_audio_play_frame(&mut self) -> Option<ffmpeg_the_third::frame::Audio> {
        let resampler_ctx = self.resampler_ctx.as_mut().unwrap();
        let mut res = ffmpeg_the_third::frame::Audio::empty();
        let raw_frame;
        {
            let mut a_frame_vec = self.audio_frame_cache_vec.write().await;
            if a_frame_vec.as_ref().unwrap().len() > 0 {
                raw_frame = a_frame_vec.as_mut().unwrap().remove(0);
            } else {
                return None;
            }
        }

        resampler_ctx.run(&raw_frame, &mut res).unwrap();
        if let Some(pts) = raw_frame.pts() {
            res.set_pts(Some(pts));
            res.set_rate(resampler_ctx.output().rate);
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
            if v_frame_vec.as_ref().unwrap().len() > 0 {
                let raw_frame = v_frame_vec.as_mut().unwrap().remove(0);
                // warn!("raw frame pts{}", raw_frame.pts().unwrap());
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
    pub fn get_end_ts(&mut self) -> i64 {
        // if self.end_audio_ts == 0 {
        //     let adur_ts = {
        //         let dur = self.format_duration.read().await;
        //         let adur_ts = *dur * self.audio_time_base.denominator() as i64
        //             / self.audio_time_base.numerator() as i64
        //             / 1000_000;
        //         self.end_audio_ts = adur_ts;
        //         warn!("audio end ts:{}", adur_ts);
        //         adur_ts
        //     };
        //     self.compute_and_set_end_time_str(adur_ts);
        // }
        // self.end_audio_ts
        self.end_timestamp
    }
    /// seek the input to a selected timestamp
    /// use the ffi function to enable seek all the frames
    /// the ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY flag makes sure
    /// the seek would go as I want, to an exact frame
    pub fn seek_timestamp_to_decode(&mut self, ts: i64) {
        {
            let mut audio_packet_cache_vec =
                self.async_rt.block_on(self.audio_packet_cache_vec.write());
            let mut video_packet_cache_vec =
                self.async_rt.block_on(self.video_packet_cache_vec.write());
            let mut audio_cache_vec = self.async_rt.block_on(self.audio_frame_cache_vec.write());
            let mut video_cache_vec = self.async_rt.block_on(self.video_frame_cache_vec.write());
            audio_packet_cache_vec.as_mut().unwrap().clear();
            video_packet_cache_vec.as_mut().unwrap().clear();
            audio_cache_vec.as_mut().unwrap().clear();
            video_cache_vec.as_mut().unwrap().clear();

            let mut input = self.async_rt.block_on(self.format_input.write());
            let main_stream_idx = {
                if let MainStream::Audio = self.main_stream {
                    *self.async_rt.block_on(self.audio_stream_index.read())
                } else {
                    *self.async_rt.block_on(self.video_stream_index.read())
                }
            };
            let main_stream_time_base = {
                if let MainStream::Audio = self.main_stream {
                    &self.audio_time_base
                } else {
                    &self.video_time_base
                }
            };
            unsafe {
                warn!("seek timestamp:{}", ts);
                let res = ffmpeg_the_third::ffi::avformat_seek_file(
                    input.as_mut().unwrap().0.as_mut_ptr(),
                    main_stream_idx as i32,
                    ts - 1 * main_stream_time_base.denominator() as i64
                        / main_stream_time_base.numerator() as i64,
                    ts,
                    ts + 1 * main_stream_time_base.denominator() as i64
                        / main_stream_time_base.numerator() as i64,
                    ffmpeg_the_third::ffi::AVSEEK_FLAG_ANY,
                );
                if res != 0 {
                    warn!("seek err num:{res}");
                }
            }
            self.async_rt.block_on(self.flush_decoders());
        }
    }
    /// use the file detail to compute the video duration and make str to inform the user
    fn compute_and_set_end_time_str(&mut self, end_ts: i64) {
        let sec_num = {
            if let MainStream::Audio = self.main_stream {
                end_ts * self.audio_time_base.numerator() as i64
                    / self.audio_time_base.denominator() as i64
            } else {
                end_ts * self.video_time_base.numerator() as i64
                    / self.video_time_base.denominator() as i64
            }
        };
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
    pub fn get_main_stream(&self) -> &MainStream {
        &self.main_stream
    }
    pub fn get_video_stream_idx(&self) -> Arc<RwLock<usize>> {
        self.video_stream_index.clone()
    }
    async fn stop_demux_and_decode(&mut self) {
        *self.demux_exit_flag.write().await = true;
        self.demux_task_handle.as_mut().unwrap().await.unwrap();
        *self.decode_exit_flag.write().await = true;
        self.decode_task_handle.as_mut().unwrap().await.unwrap();
        warn!("demux and decode task exit gracefully!!!");
    }
    pub async fn flush_decoders(&self) {
        let mut a_decoder = self.audio_decoder.write().await;
        if let Some(a) = &mut *a_decoder {
            a.0.flush();
        }
        let mut v_decoder = self.video_decoder.write().await;
        if let Some(v) = &mut *v_decoder {
            v.0.flush();
        }
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
        let codec_ctx =
            ffmpeg_the_third::codec::Context::from_parameters(stream.parameters()).unwrap();
        let mut decoder = codec_ctx.decoder().video().unwrap();
        unsafe {
            let hw_config = avcodec_get_hw_config(decoder.codec().as_ref().unwrap().as_ptr(), 0);

            if hw_config.is_null() {
                warn!("currently dont support hardware accelerate");
                return decoder;
            } else {
                let mut hw_device_ctx: *mut AVBufferRef = null_mut();
                if 0 != av_hwdevice_ctx_create(
                    &mut hw_device_ctx as *mut *mut AVBufferRef,
                    (*hw_config).device_type,
                    null(),
                    null_mut(),
                    0,
                ) {
                    warn!("hw device create err");
                    return decoder;
                }

                (*decoder.0.as_mut_ptr()).hw_device_ctx = hw_device_ctx;

                *self.hardware_config.write().await = true;
                decoder
            }
        }
    }
}
impl Drop for TinyDecoder {
    fn drop(&mut self) {
        if self.demux_task_handle.is_some() {
            let runtime = self.async_rt.clone();
            runtime.block_on(self.stop_demux_and_decode());
        }
    }
}
