use std::{
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use eframe::glow::{self, HasContext, Texture};
use egui::{
    Color32, ColorImage, CornerRadius, Id, Image, ImageData, ImageSource, Pos2, Rect, Stroke,
    TextureId, TextureOptions,
    epaint::{ImageDelta, RectShape},
    load::TextureLoader,
};
use ffmpeg_next::{codec::video, frame::Video};
const VIDEO_FILE_IMAGE: &[u8] =
    include_bytes!("D:/rustprojects/tiny_player/resources/video_file_image.png");

pub struct AppUi {
    video_file_image_bytes: Option<egui::load::Bytes>,
    video_texture_id: Option<TextureId>,
    video_rect_shape: Option<RectShape>,
    tiny_decoder: Option<crate::decode::TinyDecoder>,
    audio_player: Option<crate::audio_play::AudioPlayer>,
    current_video_frame:Option<ffmpeg_next::frame::Video>,
    video_frame_index: i32,
    current_video_frame_timestamp: i64,
    audio_frame_index: i32,
    audio_frame_timestamp: i64,
    color_image: Option<ColorImage>,
    next_frame_show_instant: Instant,
}
impl eframe::App for AppUi {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top(Id::new("top panel"))
            .default_height(200.0)
            .show(ctx, |ui| {
                //     ctx.layer_painter(ui.layer_id()).set_opacity(1.0);
                let video_file_image_source = ImageSource::Bytes {
                    uri: std::borrow::Cow::from("bytes://video_file_image.png"),
                    bytes: self.video_file_image_bytes.as_ref().unwrap().clone(),
                };
                let mut video_file_image = egui::Image::new(video_file_image_source);
                video_file_image = video_file_image.max_width(50.0 as f32);
                video_file_image = video_file_image.max_height(50.0 as f32);
                let mut file_image_button = egui::ImageButton::new(video_file_image);
                file_image_button = file_image_button.corner_radius(CornerRadius::from(15));
                if ui.add(file_image_button).clicked() {
                    let path = rfd::FileDialog::new()
                        .pick_file()
                        .filter(|f| {
                            return f.display().to_string().ends_with(".mp4");
                        })
                        .unwrap();
                    println!("filepath{}", path.display().to_string());
                    let tiny_decoder = self.tiny_decoder.as_mut().unwrap();
                    let format_input = tiny_decoder.get_input_par();
                    if format_input.is_none() {
                        tiny_decoder.set_file_path_and_init_par(&path);
                        tiny_decoder.start_process_threads();
                        // println!("video_time_base=={},audio_time_base==={}",video_time_base,audio_time_base);
                    } else {
                    }
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let format_input = self.tiny_decoder.as_ref().unwrap().get_input_par();
            if format_input.is_some() {
                let tiny_decoder = self.tiny_decoder.as_mut().unwrap();
                let now = Instant::now();
                if now
                    .checked_duration_since(self.next_frame_show_instant.clone())
                    .is_some()
                {
                        let frame_opt = tiny_decoder.get_one_video_play_frame();
                    if  frame_opt.is_some(){
                        let video_frame=frame_opt.unwrap();
                        let frame_rate = tiny_decoder.get_video_frame_rate();
                        let time_base = tiny_decoder.get_time_base();
                        //     println!("video_frame_position==={}", video_frame.packet().position);
                        if self.video_texture_id.is_none() {
                            /*
                            创建视频显示用texture
                             */
                            let color_image = ColorImage::from_rgba_unmultiplied(
                                [video_frame.width() as usize, video_frame.height() as usize],
                                video_frame.data(0),
                            );
                            let id = ctx.tex_manager().write().alloc(
                                "video_texture".to_string(),
                                ImageData::Color(Arc::new(color_image.clone())),
                                TextureOptions::LINEAR,
                            );
                            self.color_image = Some(color_image);
                            self.video_texture_id = Some(id);
                        } else {
                            // println!("duration==={}",now.checked_duration_since(self.next_frame_show_instant.clone()).unwrap().as_millis()); 
                            let now = Instant::now();
                            //     println!("next frame ins==={}",(1.0 / (frame_rate as f32) * 1000.0) as u64);
                            self.next_frame_show_instant = now
                                .checked_add(Duration::from_millis(
                                    (1.0 / (frame_rate as f32) * 1000.0) as u64,
                                ))
                                .unwrap();

                            self.video_frame_index += 1;
                            self.current_video_frame_timestamp =
                                (self.video_frame_index * time_base / frame_rate as i32) as i64;
                        }
                        self.color_image.as_mut().unwrap()
                                .as_raw_mut()
                                .copy_from_slice(video_frame.data(0));
                        let layer_painter = ctx.layer_painter(ui.layer_id());
                        // layer_painter.add(self.video_rect_shape.as_ref().unwrap().clone());
                        ctx.tex_manager().write().set(
                                self.video_texture_id.as_ref().unwrap().clone(),
                                ImageDelta::full(
                                    ImageData::Color(Arc::new(self.color_image.as_ref().unwrap().clone())),
                                    TextureOptions::LINEAR,
                                ),
                            );
                        layer_painter.image(
                            self.video_texture_id.as_ref().unwrap().clone(),
                            Rect::from_min_max(
                                Pos2::new(0.0, 50.0),
                                Pos2::new(ui.max_rect().width(), ui.max_rect().height()),
                            ),
                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );

                        // println!("painter layer height{}", ui.max_rect().height());
                        self.current_video_frame=Some(video_frame);
                    }
                }else{
                        self.color_image.as_mut().unwrap()
                                .as_raw_mut()
                                .copy_from_slice(self.current_video_frame.as_ref().unwrap().data(0));
                        let layer_painter = ctx.layer_painter(ui.layer_id());
                        // layer_painter.add(self.video_rect_shape.as_ref().unwrap().clone());
                        ctx.tex_manager().write().set(
                                self.video_texture_id.as_ref().unwrap().clone(),
                                ImageDelta::full(
                                    ImageData::Color(Arc::new(self.color_image.as_ref().unwrap().clone())),
                                    TextureOptions::LINEAR,
                                ),
                            );
                        layer_painter.image(
                            self.video_texture_id.as_ref().unwrap().clone(),
                            Rect::from_min_max(
                                Pos2::new(0.0, 50.0),
                                Pos2::new(ui.max_rect().width(), ui.max_rect().height()),
                            ),
                            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                }
                loop {
                    let audio_rate = tiny_decoder.get_resampler().output().rate;
                    let time_base = tiny_decoder.get_time_base();
                //     if self.audio_frame_timestamp 
                //         > self.current_video_frame_timestamp + (0.3*time_base as f32) as i64
                //     {
                //         // println!("audio_frame_rate{}",audio_frame_rate);
                //         break;
                //     }
                    if let Some(audio_frame) = tiny_decoder.get_one_audio_play_frame() {
                        self.audio_frame_index += 1;
                        self.audio_frame_timestamp +=
                            (audio_frame.samples() as i32 * time_base / audio_rate as i32) as i64;
                        let audio_player = self.audio_player.as_ref().unwrap();
                        audio_player.play_raw_data_from_audio_frame(audio_frame);
                        // audio_player.sync_play_time((self.current_video_frame_timestamp
                        //     / time_base as i64) as u64);
                        // println!("video_timestamp=={}\n,audio_timestamp=={}\nvideo_time_base=={}\naudio_time_rate=={}",
                        //         self.current_video_frame_timestamp,self.audio_frame_timestamp
                        //         ,(time_base as i64),(audio_rate as i64));
                    }else{
                        break;
                    }
                }
            }
            //     println!("ui width==={},uiheight==={}",ui.max_rect().width(),ui.max_rect().height());
        });
        let end_ins = std::time::Instant::now();
        // println!("loop_time==={}",(end_ins-self.frame_end_instant).as_millis());
        // self.frame_end_instant = end_ins;
        ctx.request_repaint();
    }
}
impl AppUi {
    pub fn new() -> Self {
        return Self {
            video_file_image_bytes: None,
            video_texture_id: None,
            video_rect_shape: None,
            tiny_decoder: None,
            audio_player: None,
            current_video_frame_timestamp: 0,
            video_frame_index: 0,
            current_video_frame:None,
            audio_frame_index: 0,
            audio_frame_timestamp: 0,
            color_image: None,
            next_frame_show_instant: Instant::now(),
        };
    }
    pub fn init_appui_and_resources(&mut self) {
        let mut tiny_decoder = crate::decode::TinyDecoder::new();
        let mut audio_player = crate::audio_play::AudioPlayer::new();
        audio_player.init_device();
        self.tiny_decoder = Some(tiny_decoder);
        self.audio_player = Some(audio_player);
        self.video_file_image_bytes = Some(egui::load::Bytes::Static(VIDEO_FILE_IMAGE));
    }
}
