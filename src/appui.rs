use std::{
    any::Any,
    ops::RangeInclusive,
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use eframe::glow::{self, HasContext, Texture};
use egui::{
    epaint::{ImageDelta, RectShape}, style::{self, HandleShape}, Color32, ColorImage, CornerRadius, Image, ImageData, ImageSource, Pos2, Rect, Shape, TextureId, TextureOptions, Ui, UiBuilder
};
use ffmpeg_next::{codec::video, frame::Video};
use log::{info, warn};
const VIDEO_FILE_IMAGE: &[u8] =
    include_bytes!("D:/rustprojects/tiny_player/resources/video_file_image.png");

pub struct AppUi {
    video_file_image_bytes: Option<egui::load::Bytes>,
    video_texture_id: Option<TextureId>,
    tiny_decoder: Option<crate::decode::TinyDecoder>,
    audio_player: Option<crate::audio_play::AudioPlayer>,
    video_frame_index: i32,
    current_video_frame_timestamp: i64,
    audio_frame_index: i32,
    audio_frame_timestamp: i64,
    color_image: Option<ColorImage>,
    next_frame_show_instant: Instant,
    pause_flag: bool,
}
impl eframe::App for AppUi {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(format_input) = self.tiny_decoder.as_ref().unwrap().get_input_par() {
                if !self.pause_flag {
                    let now = Instant::now();
                    /*
                    if now is next_frame_time or a little beyond get and show a new frame
                     */
                    if now
                        .checked_duration_since(self.next_frame_show_instant.clone())
                        .is_some()
                    {
                        let tiny_decoder = self.tiny_decoder.as_mut().unwrap();
                        if let Some(video_frame) = tiny_decoder.get_one_video_play_frame() {
                            let frame_rate = tiny_decoder.get_video_frame_rate();
                            let time_base = tiny_decoder.get_time_base();
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
                                self.next_frame_show_instant = now
                                    .checked_add(Duration::from_millis(
                                        (1.0 / (frame_rate as f32) * 1000.0) as u64,
                                    ))
                                    .unwrap();
                                self.color_image = Some(color_image);
                                self.video_texture_id = Some(id);
                            } else {
                                self.next_frame_show_instant = self
                                    .next_frame_show_instant
                                    .checked_add(Duration::from_millis(
                                        (1.0 / (frame_rate as f32) * 1000.0) as u64,
                                    ))
                                    .unwrap();

                                self.video_frame_index += 1;
                                self.current_video_frame_timestamp =
                                    (self.video_frame_index * time_base / frame_rate as i32) as i64;
                            }
                            self.color_image
                                .as_mut()
                                .unwrap()
                                .as_raw_mut()
                                .copy_from_slice(video_frame.data(0));
                            ctx.tex_manager().write().set(
                                self.video_texture_id.as_ref().unwrap().clone(),
                                ImageDelta::full(
                                    ImageData::Color(Arc::new(
                                        self.color_image.as_ref().unwrap().clone(),
                                    )),
                                    TextureOptions::LINEAR,
                                ),
                            );
                            self.paint_video_image(ctx, ui);
                        }
                    } else {
                        /*
                        if time didnt met next_frame_time,show the previous frame
                         */
                        self.paint_video_image(ctx, ui);
                    }
                    /*
                    add audio frame data to the audio player and sync with video frame time
                     */
                    loop {
                        let tiny_decoder = self.tiny_decoder.as_mut().unwrap();
                        let audio_rate = tiny_decoder.get_resampler().output().rate;
                        let time_base = tiny_decoder.get_time_base();
                        let audio_player = self.audio_player.as_ref().unwrap();
                        if self.audio_frame_timestamp > self.current_video_frame_timestamp {
                            audio_player.sync_play_time();
                            break;
                        }
                        if let Some(audio_frame) = tiny_decoder.get_one_audio_play_frame() {
                            self.audio_frame_index += 1;
                            // warn!("one source time==={}",(audio_frame.samples() as f32 /audio_rate as f32)*1000.0);
                            self.audio_frame_timestamp = self.audio_frame_timestamp
                                + (audio_frame.samples() as i32 * time_base / audio_rate as i32)
                                    as i64;
                            audio_player.play_raw_data_from_audio_frame(audio_frame);
                        } else {
                            break;
                        }
                    }
                } else {
                    self.next_frame_show_instant = Instant::now();
                    self.paint_video_image(ctx, ui);
                }
            }
            ui.vertical(|ui| {
                let outer_ui_width=ui.max_rect().width();
                let outer_ui_height=ui.max_rect().height();
                let video_file_image_source = ImageSource::Bytes {
                    uri: std::borrow::Cow::from("bytes://video_file_image.png"),
                    bytes: self.video_file_image_bytes.as_ref().unwrap().clone(),
                };
                let mut video_file_image = egui::Image::new(video_file_image_source);
                video_file_image = video_file_image.max_width(50.0 as f32);
                video_file_image = video_file_image.max_height(50.0 as f32);
                let mut file_image_button = egui::ImageButton::new(video_file_image);
                file_image_button = file_image_button.corner_radius(CornerRadius::from(15));
                ui.set_opacity(0.5);
                if ui.add(file_image_button).clicked() {
                    let path = rfd::FileDialog::new()
                        .pick_file()
                        .filter(|f| {
                            return f.display().to_string().ends_with(".mp4");
                        })
                        .unwrap();
                    warn!("filepath{}", path.display().to_string());
                    let tiny_decoder = self.tiny_decoder.as_mut().unwrap();
                    let format_input = tiny_decoder.get_input_par();
                    if format_input.is_none() {
                        tiny_decoder.set_file_path_and_init_par(&path);
                        tiny_decoder.start_process_threads();
                    } else {
                    }
                }
                ui.horizontal(|ui| {
                    ui.set_height(outer_ui_height-100.0);
                    let video_file_image_source = ImageSource::Bytes {
                        uri: std::borrow::Cow::from("bytes://video_file_image.png"),
                        bytes: self.video_file_image_bytes.as_ref().unwrap().clone(),
                    };
                    let mut video_file_image = egui::Image::new(video_file_image_source);
                    video_file_image = video_file_image.max_width(50.0 as f32);
                    video_file_image = video_file_image.max_height(50.0 as f32);
                    let mut pause_btn = egui::ImageButton::new(video_file_image.clone());
                    ui.centered_and_justified(|ui| {
                        if ui.add(pause_btn).clicked() {
                            self.pause_flag = !self.pause_flag;
                        }
                    });
                });
                // ui.add_space(ui.max_rect().height()/2.0-50.0);
                ui.horizontal(|ui| {
                        // ui.set_width(outer_ui_width-100.0);
                    let mut n = 0;
                    let mut progress_slider = egui::Slider::new(&mut n, RangeInclusive::new(0, 100));
                    let mut slider_width_style=style::Style::default();
                    slider_width_style.spacing.slider_width=outer_ui_width-120.0;
                    ui.set_style(slider_width_style);
                    if ui.add(progress_slider).changed() {
                        warn!("slider dragged!");
                    }
                });
            });

            ctx.request_repaint();
        });
    }
}
impl AppUi {
    pub fn new() -> Self {
        return Self {
            video_file_image_bytes: None,
            video_texture_id: None,
            tiny_decoder: None,
            audio_player: None,
            current_video_frame_timestamp: 0,
            video_frame_index: 0,
            audio_frame_index: 0,
            audio_frame_timestamp: 0,
            color_image: None,
            next_frame_show_instant: Instant::now(),
            pause_flag: false,
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
    fn paint_video_image(&mut self, ctx: &egui::Context, ui: &mut Ui) {
        /*
        show image that contains the video texture
         */
        let layer_painter = ctx.layer_painter(ui.layer_id());
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
}
