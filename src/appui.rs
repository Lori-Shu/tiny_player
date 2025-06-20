use std::{
    ops::RangeInclusive,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use eframe::glow::{self, HasContext, Texture};
use egui::{
    Align, Align2, Color32, ColorImage, CornerRadius, Galley, Image, ImageData, ImageSource, Pos2,
    Rect, RichText, Shape, TextBuffer, TextStyle, TextureId, TextureOptions, Ui, UiBuilder,
    WidgetText,
    epaint::{ImageDelta, RectShape},
    include_image,
    style::{self, HandleShape},
};

use ffmpeg_the_third::frame::Video;
use log::{info, warn};
use time::format_description;

use crate::decode::TinyDecoder;
const VIDEO_FILE_IMG: ImageSource =
    include_image!("D:/rustprojects/tiny_player/resources/video_file_img.png");
const VOLUMN_IMG: ImageSource =
    include_image!("D:/rustprojects/tiny_player/resources/volumn_img.png");
const PLAY_IMG: ImageSource = include_image!("D:/rustprojects/tiny_player/resources/play_img.png");
const PAUSE_IMG: ImageSource =
    include_image!("D:/rustprojects/tiny_player/resources/pause_img.png");
const FULLSCREEN_IMG: ImageSource =
    include_image!("D:/rustprojects/tiny_player/resources/fullscreen_img.png");
pub struct AppUi {
    video_texture_id: Option<TextureId>,
    tiny_decoder: Option<crate::decode::TinyDecoder>,
    audio_player: Option<crate::audio_play::AudioPlayer>,
    current_video_frame_timestamp: Arc<Mutex<i64>>,
    audio_frame_timestamp: i64,
    color_image: Option<ColorImage>,
    next_frame_show_instant: Instant,
    pause_flag: bool,
    play_time: time::Time,
    time_text: String,
    fullscreen_flag: bool,
    control_ui_flag: bool,
    last_show_control_ui_instant: Instant,
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
                                self.next_frame_show_instant = now
                                    .checked_add(Duration::from_millis(
                                        (1.0 / (frame_rate as f32) * 1000.0) as u64,
                                    ))
                                    .unwrap();
                                {
                                    let mut lock_guard =
                                        self.current_video_frame_timestamp.lock().unwrap();
                                    (*lock_guard) += (1 * time_base / frame_rate as i32) as i64;
                                }
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
                        audio_player.sync_play_time();
                        if let Some(audio_frame) = tiny_decoder.get_one_audio_play_frame() {
                            if let Some(ts) = audio_frame.timestamp() {
                                warn!("audio frame ts==={}", ts);
                            }
                            //     self.audio_frame_timestamp = *mutex_guard
                            //         + (audio_frame.samples() as i32 * time_base / audio_rate as i32)
                            //             as i64;
                            //     warn!("audio_frame_ts{}",self.audio_frame_timestamp);
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

            let vertical_ui_response = ui.vertical(|ui| {
                if self.control_ui_flag {
                    ui.set_opacity(0.8);
                } else {
                    ui.set_opacity(0.0);
                }
                let mut video_file_image = egui::Image::new(VIDEO_FILE_IMG);
                video_file_image = video_file_image.max_height(50.0 as f32);
                let mut file_image_button = egui::ImageButton::new(video_file_image);
                file_image_button = file_image_button.corner_radius(CornerRadius::from(15));
                let file_img_btn_response = ui.add(file_image_button);
                if file_img_btn_response.hovered() {
                    self.control_ui_flag = true;
                    self.last_show_control_ui_instant = Instant::now();
                }
                if file_img_btn_response.clicked() {
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
                    let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
                    if let Some(input_par) = tiny_decoder.get_input_par() {
                        ui.set_height(ctx.screen_rect().height() - 220.0);
                        let mut play_or_pause_image_source;
                        if self.pause_flag {
                            play_or_pause_image_source = PLAY_IMG;
                        } else {
                            play_or_pause_image_source = PAUSE_IMG;
                        }
                        let mut play_or_pause_image = egui::Image::new(play_or_pause_image_source);
                        play_or_pause_image = play_or_pause_image.max_width(200.0 as f32);
                        play_or_pause_image = play_or_pause_image.max_height(200.0 as f32);
                        let mut play_or_pause_btn = egui::ImageButton::new(play_or_pause_image);
                        play_or_pause_btn = play_or_pause_btn.corner_radius(80.0);
                        ui.add_space(ctx.screen_rect().width() / 2.0 - 100.0);
                        let btn_response = ui.add(play_or_pause_btn);
                        if btn_response.hovered() {
                            self.control_ui_flag = true;
                            self.last_show_control_ui_instant = Instant::now();
                        }
                        if btn_response.clicked() {
                            self.pause_flag = !self.pause_flag;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
                    if let Some(input_par) = tiny_decoder.get_input_par() {
                        ui.add_space(ctx.screen_rect().width() - 120.0);
                        let audio_player = self.audio_player.as_mut().unwrap();
                        let mut volumn_slider = egui::Slider::new(
                            &mut audio_player.current_volumn,
                            RangeInclusive::new(0.0, 2.0),
                        );
                        volumn_slider = volumn_slider.vertical();
                        volumn_slider = volumn_slider.show_value(false);
                        let mut slider_style = style::Style::default();
                        slider_style.spacing.slider_width = 70.0;
                        ui.set_style(slider_style);
                        let mut slider_response = ui.add(volumn_slider);
                        slider_response = slider_response
                            .on_hover_text((audio_player.current_volumn * 100.0).to_string());
                        if slider_response.hovered() {
                            self.control_ui_flag = true;
                            self.last_show_control_ui_instant = Instant::now();
                        }
                        if slider_response.changed() {
                            warn!("volumn slider dragged!");
                            audio_player.change_volumn();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    self.update_time_and_time_text();
                    let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
                    if let Some(input_par) = tiny_decoder.get_input_par() {
                        ui.set_height(50.0);
                        {
                            let mut mutex_guard =
                                self.current_video_frame_timestamp.lock().unwrap();
                            let mut progress_slider = egui::Slider::new(
                                &mut *mutex_guard,
                                RangeInclusive::new(
                                    0,
                                    tiny_decoder.get_total_video_frames()
                                        * tiny_decoder.get_time_base() as i64
                                        / tiny_decoder.get_video_frame_rate() as i64,
                                ),
                            );
                            progress_slider = progress_slider.show_value(false);
                            progress_slider = progress_slider.text(WidgetText::RichText(
                                RichText::new(self.time_text.clone())
                                    .size(20.0)
                                    .color(Color32::BROWN),
                            ));
                            let mut slider_width_style = style::Style::default();
                            slider_width_style.spacing.slider_width =
                                ctx.screen_rect().width() - 300.0;
                            slider_width_style.spacing.slider_rail_height = 10.0;
                            ui.set_style(slider_width_style);
                            let slider_response = ui.add(progress_slider);
                            if slider_response.hovered() {
                                self.control_ui_flag = true;
                                self.last_show_control_ui_instant = Instant::now();
                            }
                            if slider_response.changed() {
                                warn!("slider dragged!");
                                self.audio_frame_timestamp = *mutex_guard;
                                tiny_decoder.seek_timestamp_to_decode(*mutex_guard);
                                // self.audio_player.as_ref().unwrap().source_queue_skip_to_end();
                            }
                        }
                        let mut volumn_img = egui::Image::new(VOLUMN_IMG);
                        volumn_img = volumn_img.max_height(20.0);
                        let volumn_img_btn = egui::ImageButton::new(volumn_img);
                        let btn_response = ui.add(volumn_img_btn);
                        if btn_response.hovered() {
                            self.control_ui_flag = true;
                            self.last_show_control_ui_instant = Instant::now();
                        }
                        // if btn_response.clicked() {
                        //     let audio_player = self.audio_player.as_ref().unwrap();

                        // }
                        let mut fullscreen_img = egui::Image::new(FULLSCREEN_IMG);
                        fullscreen_img = fullscreen_img.max_height(20.0);
                        let fullscreen_image_btn = egui::ImageButton::new(fullscreen_img);
                        let btn_response = ui.add(fullscreen_image_btn);
                        if btn_response.hovered() {
                            self.control_ui_flag = true;
                            self.last_show_control_ui_instant = Instant::now();
                        }
                        if btn_response.clicked() {
                            self.fullscreen_flag = !self.fullscreen_flag;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                self.fullscreen_flag,
                            ));
                        }
                    }
                });
            });
            let now = Instant::now();
            if (now - self.last_show_control_ui_instant).as_secs() > 3 {
                self.control_ui_flag = false;
            }

            ctx.request_repaint();
        });
    }
}
impl AppUi {
    pub fn new() -> Self {
        return Self {
            video_texture_id: None,
            tiny_decoder: None,
            audio_player: None,
            current_video_frame_timestamp: Arc::new(Mutex::new(0)),
            audio_frame_timestamp: 0,
            play_time: time::Time::from_hms(0, 0, 0).unwrap(),
            color_image: None,
            next_frame_show_instant: Instant::now(),
            pause_flag: false,
            time_text: String::new(),
            fullscreen_flag: false,
            control_ui_flag: true,
            last_show_control_ui_instant: Instant::now(),
        };
    }
    pub fn init_appui_and_resources(&mut self) {
        let mut tiny_decoder =
            crate::decode::TinyDecoder::new(self.current_video_frame_timestamp.clone());
        let mut audio_player = crate::audio_play::AudioPlayer::new();
        audio_player.init_device();
        self.tiny_decoder = Some(tiny_decoder);
        self.audio_player = Some(audio_player);
    }
    fn paint_video_image(&mut self, ctx: &egui::Context, ui: &mut Ui) {
        /*
        show image that contains the video texture
         */
        let layer_painter = ctx.layer_painter(ui.layer_id());
        layer_painter.image(
            self.video_texture_id.as_ref().unwrap().clone(),
            Rect::from_min_max(
                Pos2::new(0.0, 0.0),
                Pos2::new(ctx.screen_rect().width(), ctx.screen_rect().height()),
            ),
            Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    fn update_time_and_time_text(&mut self) {
        let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
        if let Some(input_par) = tiny_decoder.get_input_par() {
            let sec_num;
            {
                let mutex_guard = self.current_video_frame_timestamp.lock().unwrap();
                sec_num = *mutex_guard / tiny_decoder.get_time_base() as i64;
            }

            let sec = (sec_num % 60) as u8;
            let min_num = sec_num / 60;
            let min = (min_num % 60) as u8;
            let hour_num = min_num / 60;
            let hour = hour_num as u8;
            let time = time::Time::from_hms(hour, min, sec).unwrap();
            if !time.eq(&self.play_time) {
                let formatter = format_description::parse("[hour]:[minute]:[second]").unwrap();
                let mut now_str = time.format(&formatter).unwrap();
                now_str.push_str("|");
                now_str.push_str(tiny_decoder.get_total_video_time_formatted_string());
                self.time_text = now_str;
                self.play_time = time;
            }
        }
    }
}
