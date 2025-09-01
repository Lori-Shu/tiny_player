use std::{
    ops::RangeInclusive,
    path::Path,
    str::FromStr,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use egui::{
    Color32, ColorImage, Context, CornerRadius, ImageData, ImageSource, Pos2, Rect, RichText,
    TextureId, TextureOptions, Ui, Vec2, Vec2b, ViewportBuilder, ViewportId, WidgetText,
    epaint::ImageDelta, include_image,
};

use image::DynamicImage;
use log::warn;
use reqwest::{Method, Url};
use time::format_description;

use crate::asyncmod::{AsyncContext, SocketChatMessage};

const VIDEO_FILE_IMG: ImageSource = include_image!("../resources/video_file_img.png");
const VOLUMN_IMG: ImageSource = include_image!("../resources/volumn_img.png");
const PLAY_IMG: ImageSource = include_image!("../resources/play_img.png");
const PAUSE_IMG: ImageSource = include_image!("../resources/pause_img.png");
const FULLSCREEN_IMG: ImageSource = include_image!("../resources/fullscreen_img.png");
const DEFAULT_BG_IMG: ImageSource = include_image!("../resources/background.png");
const MAPLE_FONT: &[u8] = include_bytes!("../resources/fonts/MapleMono-CN-Regular.ttf");
const EMOJI_FONT: &[u8] = include_bytes!("../resources/fonts/seguiemj.ttf");
struct CurrentVideoFrameDetail {
    frame: ffmpeg_the_third::util::frame::Video,
    pts: i64,
}
pub struct AppUi {
    video_texture_id: Option<TextureId>,
    tiny_decoder: Option<crate::decode::TinyDecoder>,
    audio_player: Option<crate::audio_play::AudioPlayer>,
    current_audio_frame_timestamp: Arc<RwLock<i64>>,
    main_color_image: Option<ColorImage>,
    bg_dyn_img:DynamicImage,
    frame_show_instant: Instant,
    pause_flag: bool,
    play_time: time::Time,
    time_text: String,
    fullscreen_flag: bool,
    control_ui_flag: bool,
    err_window_flag: bool,
    err_window_msg: String,
    last_show_control_ui_instant: Instant,
    app_start_instant: Instant,
    current_frame_detail: Option<CurrentVideoFrameDetail>,
    async_ctx: AsyncContext,
    show_content_window_flag: bool,
    content_str: String,
    opened_file: Option<std::path::PathBuf>,
    open_file_dialog: Option<egui_file::FileDialog>,
    username_buf: String,
}
impl eframe::App for AppUi {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                /*
                down part is update data part with no ui painting

                 */
                let now = Instant::now();
                if let None = self.video_texture_id {
                    self.create_video_texture(ctx);
                    self.frame_show_instant = now;
                }
                if let Some(_input) = self.tiny_decoder.as_ref().unwrap().get_input_par() {
                    if !self.pause_flag {
                        /*
                        if now is next_frame_time or a little beyond get and show a new frame
                         */
                        if let Ok(_keepawake) = keepawake::Builder::default()
                            .display(true)
                            .idle(true)
                            .app_name("tiny_player")
                            .reason("video play")
                            .create()
                        {
                            self.audio_source_add();
                            self.audio_pts_to_current_play_pts();
                            if now
                                .checked_duration_since(self.frame_show_instant.clone())
                                .is_some()
                            {
                                if self.check_video_wait_for_audio() {
                                    if let Some(tiny_decoder) = &mut self.tiny_decoder {
                                        if let Some(v_frame) =
                                            tiny_decoder.get_one_video_play_frame_and_pts()
                                        {
                                            if let Some(detail) = &mut self.current_frame_detail {
                                                if let Ok(cur_au_ts) =
                                                    self.current_audio_frame_timestamp.read()
                                                {
                                                    if let Some(pts) = v_frame.pts() {
                                                        if *cur_au_ts * 1000
                                                            / tiny_decoder.get_audio_time_base()
                                                                as i64
                                                            - detail.pts * 1000
                                                                / tiny_decoder.get_video_time_base()
                                                                    as i64
                                                            > 100
                                                        {
                                                            self.frame_show_instant = now;
                                                        } else {
                                                            let duration = Duration::from_millis(
                                                                ((pts - detail.pts) * 1000
                                                                    / tiny_decoder
                                                                        .get_video_time_base()
                                                                        as i64)
                                                                    as u64,
                                                            );
                                                            if duration
                                                                > Duration::from_millis(1000)
                                                                || duration
                                                                    < Duration::from_millis(0)
                                                            {
                                                                self.frame_show_instant = now;
                                                            } else if let Some(ins) = self
                                                                .frame_show_instant
                                                                .checked_add(duration)
                                                            {
                                                                self.frame_show_instant = ins;
                                                            }
                                                        }
                                                        detail.pts = pts;
                                                        detail.frame = v_frame;
                                                    }

                                                    if let Some(c_img) = &mut self.main_color_image
                                                    {
                                                        c_img
                                                            .as_raw_mut()
                                                            .copy_from_slice(detail.frame.data(0));
                                                        if let Some(v_tex) = &self.video_texture_id
                                                        {
                                                            ctx.tex_manager().write().set(
                                                                *v_tex,
                                                                ImageDelta::full(
                                                                    ImageData::Color(Arc::new(
                                                                        c_img.clone(),
                                                                    )),
                                                                    TextureOptions::LINEAR,
                                                                ),
                                                            );
                                                        }
                                                    }
                                                }
                                            } else {
                                                if let Some(pts) = v_frame.pts() {
                                                    self.current_frame_detail =
                                                        Some(CurrentVideoFrameDetail {
                                                            frame: v_frame,
                                                            pts: pts,
                                                        });
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    self.frame_show_instant = now;
                                }
                            }
                            if self.check_play_is_at_endtail() {
                                self.pause_flag = true;
                            }
                        }
                    } else {
                        /*
                        player paused
                         */
                        self.frame_show_instant = now;
                    }
                }
                /*
                down part is ui painting and control

                 */
                self.paint_video_image(ctx, ui);
                self.paint_frame_rate_text(ui, ctx, &now);
                if self.control_ui_flag {
                    ui.set_opacity(1.0);
                } else {
                    ui.set_opacity(0.0);
                }
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        self.paint_file_btn(ui, ctx, &now);
                        self.paint_open_network_content_box(ui, ctx, &now);
                    });
                });

                ui.add_space(ctx.screen_rect().height() / 2.0 - 300.0);
                ui.horizontal(|ui| {
                    self.paint_playpause_btn(ui, ctx, &now);
                });
                ui.add_space(ctx.screen_rect().height() / 2.0 - 300.0);
                ui.horizontal(|ui| {
                    let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
                    if let Some(_input_par) = tiny_decoder.get_input_par() {
                        ui.add_space(ctx.screen_rect().width() - 120.0);
                        let audio_player = self.audio_player.as_mut().unwrap();
                        let mut volumn_slider = egui::Slider::new(
                            &mut audio_player.current_volumn,
                            RangeInclusive::new(0.0, 2.0),
                        );
                        volumn_slider = volumn_slider.vertical();
                        volumn_slider = volumn_slider.show_value(false);
                        let mut slider_style = egui::style::Style::default();
                        slider_style.spacing.slider_width = 150.0;
                        ui.set_style(slider_style);
                        let mut slider_response = ui.add(volumn_slider);
                        slider_response = slider_response
                            .on_hover_text((audio_player.current_volumn * 100.0).to_string());
                        if slider_response.hovered() {
                            self.control_ui_flag = true;
                            self.last_show_control_ui_instant = now;
                        }
                        if slider_response.changed() {
                            warn!("volumn slider dragged!");
                            audio_player.change_volumn();
                        }
                    }
                });

                ui.horizontal(|ui| {
                    self.update_time_and_time_text();
                    self.paint_control_area(ui, ctx, &now);
                });
                if (now - self.last_show_control_ui_instant).as_secs() > 3 {
                    self.control_ui_flag = false;
                }
            });

            ctx.request_repaint();
        });
    }
}
impl AppUi {
    pub fn replace_fonts(&self, ctx: &egui::Context) {
        // Start with the default fonts (we will be adding to them rather than replacing them).
        let mut fonts = egui::FontDefinitions::default();

        // Install my own font (maybe supporting non-latin characters).
        // .ttf and .otf files supported.
        fonts.font_data.insert(
            "app_default_font".to_owned(),
            std::sync::Arc::new(egui::FontData::from_static(MAPLE_FONT)),
        );
        // 彩色 Emoji 字体
        fonts.font_data.insert(
            "noto_emoji".to_owned(),
            Arc::new(egui::FontData::from_static(EMOJI_FONT)),
        );
        // Put my font first (highest priority) for proportional text:
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "app_default_font".to_owned());

        // Put my font as last fallback for monospace:
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "app_default_font".to_owned());

        // 设置 fallback 顺序
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(1, "noto_emoji".to_owned());
        // Tell egui to use these fonts:
        ctx.set_fonts(fonts);
    }
    pub fn new() -> Self {
        let play_time = {
            if let Ok(play_time) = time::Time::from_hms(0, 0, 0) {
                play_time
            } else {
                panic!("err construct playtime 0:0:0");
            }
        };
        let async_ctx = AsyncContext::new();
        let f_dialog = egui_file::FileDialog::open_file(None);
        let (color_image,dyn_img) = {
            if let ImageSource::Bytes {  bytes,.. } = DEFAULT_BG_IMG {
                if let Ok(dynimg) = image::load_from_memory(&bytes) {
                    (
                        ColorImage::from_rgba_unmultiplied(
                            [dynimg.width() as usize, dynimg.height() as usize],
                            dynimg.as_bytes(),
                        ),
                        dynimg,
                    )
                } else {
                    panic!();
                }
            } else {
                panic!();
            }
        };
        let mut sel = Self {
            video_texture_id: None,
            tiny_decoder: None,
            audio_player: None,
            current_audio_frame_timestamp: Arc::new(RwLock::new(0)),
            play_time: play_time,
            main_color_image: Some(color_image),
            frame_show_instant: Instant::now(),
            pause_flag: true,
            time_text: String::new(),
            fullscreen_flag: false,
            control_ui_flag: true,
            err_window_flag: false,
            err_window_msg: String::new(),
            last_show_control_ui_instant: Instant::now(),
            app_start_instant: Instant::now(),
            current_frame_detail: None,
            async_ctx: async_ctx,
            show_content_window_flag: false,
            content_str: String::new(),
            opened_file: None,
            open_file_dialog: Some(f_dialog),
            bg_dyn_img: dyn_img,
            username_buf: String::new(),
        };
        let tiny_decoder = crate::decode::TinyDecoder::new();
        let audio_player = crate::audio_play::AudioPlayer::new();
        sel.tiny_decoder = Some(tiny_decoder);
        sel.audio_player = Some(audio_player);
        sel
    }
    fn paint_video_image(&mut self, ctx: &egui::Context, ui: &mut Ui) {
        /*
        show image that contains the video texture
         */
        let layer_painter = ctx.layer_painter(ui.layer_id());
        layer_painter.image(
            *self.video_texture_id.as_ref().unwrap(),
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
        if let Some(_input_par) = tiny_decoder.get_input_par() {
            let sec_num;
            if let Ok(play_ts) = self.current_audio_frame_timestamp.read() {
                sec_num = *play_ts / tiny_decoder.get_audio_time_base() as i64;

                let sec = (sec_num % 60) as u8;
                let min_num = sec_num / 60;
                let min = (min_num % 60) as u8;
                let hour_num = min_num / 60;
                let hour = hour_num as u8;
                if let Ok(time) = time::Time::from_hms(hour, min, sec) {
                    if !time.eq(&self.play_time) {
                        if let Ok(formatter) = format_description::parse("[hour]:[minute]:[second]")
                        {
                            if let Ok(mut now_str) = time.format(&formatter) {
                                now_str.push_str("|");
                                now_str.push_str(tiny_decoder.get_end_time_formatted_string());
                                self.time_text = now_str;
                                self.play_time = time;
                            }
                        }
                    }
                }
            }
        }
    }
    fn update_color_image(&mut self) {
        let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
        let frame_rect = tiny_decoder.get_video_frame_rect();
        let color_image = ColorImage::filled(
            [frame_rect[0] as usize, frame_rect[1] as usize],
            Color32::from_rgba_unmultiplied(0, 200, 0, 200),
        );
        if let Some(img) = &mut self.main_color_image {
            *img = color_image;
        }
    }
    fn create_video_texture(&mut self, ctx: &egui::Context) {
        /*
        创建视频显示用texture
         */
        let id = ctx.tex_manager().write().alloc(
            "video_texture".to_string(),
            ImageData::Color(Arc::new(self.main_color_image.as_ref().unwrap().clone())),
            TextureOptions::LINEAR,
        );

        self.video_texture_id = Some(id);
    }

    fn audio_source_add(&mut self) {
        /*
        add audio frame data to the audio player
         */
        loop {
            if self.check_play_is_at_endtail() {
                break;
            }
            if let Some(audio_player) = &mut self.audio_player {
                if let Some(tiny_decoder) = &mut self.tiny_decoder {
                    if audio_player.len() < 10 {
                        if let Some(audio_frame) = tiny_decoder.get_one_audio_play_frame_and_pts() {
                            if let Some(pts) = audio_frame.pts() {
                                audio_player.set_pts(pts);
                            }
                            audio_player.play_raw_data_from_audio_frame(audio_frame);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    fn paint_file_btn(&mut self, ui: &mut Ui, ctx: &Context, now: &Instant) {
        let video_file_image = egui::Image::new(VIDEO_FILE_IMG);
        let mut file_image_button = egui::ImageButton::new(video_file_image).frame(false);
        file_image_button = file_image_button.corner_radius(CornerRadius::from(40));
        let file_img_btn_response = ui.add_sized(Vec2::new(100.0, 100.0), file_image_button);
        if file_img_btn_response.hovered() {
            self.control_ui_flag = true;
            self.last_show_control_ui_instant = now.clone();
        }
        if self.err_window_flag {
            egui::Window::new("err window")
                .default_pos(Pos2::new(
                    ctx.screen_rect().width() / 2.0,
                    ctx.screen_rect().height() / 2.0,
                ))
                .show(ctx, |ui| {
                    ui.label(&self.err_window_msg);
                    if ui.button("close").clicked() {
                        self.err_window_flag = false;
                    }
                });
        }

        if let Some(d) = &mut self.open_file_dialog {
            d.show(ctx);
            if d.selected() {
                if let Some(p) = d.path() {
                    warn!("path selected{:#?}", p);
                    self.opened_file = Some(p.to_path_buf());
                }
            }
        }

        if file_img_btn_response.clicked() {
            if let Some(dialog) = &mut self.open_file_dialog {
                dialog.open();
            }
        }
        if let Some(path) = self.opened_file.take() {
            if path.display().to_string().ends_with(".mp4")
                || path.display().to_string().ends_with(".mkv")
                || path.display().to_string().ends_with(".ts")
            {
                warn!("filepath{}", path.display().to_string());
                if let Some(tiny_decoder) = &mut self.tiny_decoder {
                    let format_input = tiny_decoder.get_input_par();
                    if let Some(path_str) = path.to_str() {
                        let video_file_path = Path::new(path_str);
                        if let None = format_input {
                            self.pause_flag = true;
                            tiny_decoder.set_file_path_and_init_par(video_file_path);
                            tiny_decoder.start_process_threads();
                            self.update_color_image();
                            self.frame_show_instant = *now;
                        } else {
                            self.pause_flag = true;
                            tiny_decoder.change_file_path_and_init_par(video_file_path);
                            self.reset_main_tex_to_bg(ctx);
                            self.update_color_image();
                            if let Ok(mut mutex_guard) = self.current_audio_frame_timestamp.write()
                            {
                                *mutex_guard = 0;
                            }
                            self.current_frame_detail = None;
                            self.frame_show_instant = *now;
                            if let Some(au_pl) = &mut self.audio_player {
                                au_pl.source_queue_skip_to_end();
                            }
                        }
                    }
                }
            } else {
                self.err_window_msg = "please choose a valid file !!!".to_string();
                self.err_window_flag = true;
            }
        }
    }

    fn paint_playpause_btn(&mut self, ui: &mut Ui, ctx: &Context, now: &Instant) {
        let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
        if let Some(_input_par) = tiny_decoder.get_input_par() {
            let play_or_pause_image_source;
            if self.pause_flag {
                play_or_pause_image_source = PLAY_IMG;
            } else {
                play_or_pause_image_source = PAUSE_IMG;
            }
            let mut play_or_pause_image = egui::Image::new(play_or_pause_image_source);
            play_or_pause_image = play_or_pause_image.corner_radius(50.0);
            let play_or_pause_btn = egui::ImageButton::new(play_or_pause_image).frame(false);

            ui.add_space(ctx.screen_rect().width() / 2.0 - 100.0);
            let btn_response = ui.add_sized(Vec2::new(200.0, 200.0), play_or_pause_btn);
            if btn_response.hovered() {
                self.control_ui_flag = true;
                self.last_show_control_ui_instant = now.clone();
            }
            if btn_response.clicked() {
                self.pause_flag = !self.pause_flag;
                let audio_player = self.audio_player.as_ref().unwrap();
                if self.pause_flag {
                    audio_player.pause_play();
                } else {
                    audio_player.continue_play();
                }
            }
        }
    }

    fn paint_control_area(&mut self, ui: &mut Ui, ctx: &Context, now: &Instant) {
        let tiny_decoder = self.tiny_decoder.as_ref().unwrap();
        if let Some(_input_par) = tiny_decoder.get_input_par() {
            if let Ok(mut timestamp) = self.current_audio_frame_timestamp.write() {
                let mut progress_slider = egui::Slider::new(
                    &mut *timestamp,
                    RangeInclusive::new(0, tiny_decoder.get_end_audio_ts()),
                );
                progress_slider = progress_slider.show_value(false);
                progress_slider = progress_slider.text(WidgetText::RichText(Arc::new(
                    RichText::new(self.time_text.clone())
                        .size(20.0)
                        .color(Color32::BROWN),
                )));
                let mut slider_width_style = egui::style::Style::default();
                slider_width_style.spacing.slider_width = ctx.screen_rect().width() - 350.0;
                slider_width_style.spacing.slider_rail_height = 10.0;
                ui.set_style(slider_width_style);
                let slider_response = ui.add(progress_slider);
                if slider_response.hovered() {
                    self.control_ui_flag = true;
                    self.last_show_control_ui_instant = Instant::now();
                }
                if slider_response.changed() {
                    warn!("slider dragged!");
                    let audio_player = self.audio_player.as_mut().unwrap();
                    audio_player.source_queue_skip_to_end();
                    self.frame_show_instant = now.clone();
                    self.current_frame_detail = None;
                    tiny_decoder.seek_timestamp_to_decode(*timestamp);
                }
            }
            let mut volumn_img = egui::Image::new(VOLUMN_IMG);
            volumn_img = volumn_img.max_height(20.0);
            volumn_img = volumn_img.corner_radius(50.0);
            let volumn_img_btn = egui::ImageButton::new(volumn_img).frame(false);
            let btn_response = ui.add(volumn_img_btn);
            if btn_response.hovered() {
                self.control_ui_flag = true;
                self.last_show_control_ui_instant = now.clone();
            }
            let mut fullscreen_img = egui::Image::new(FULLSCREEN_IMG);
            fullscreen_img = fullscreen_img.max_height(20.0);
            fullscreen_img = fullscreen_img.corner_radius(50.0);
            let fullscreen_image_btn = egui::ImageButton::new(fullscreen_img).frame(false);
            let btn_response = ui.add(fullscreen_image_btn);
            if btn_response.hovered() {
                self.control_ui_flag = true;
                self.last_show_control_ui_instant = now.clone();
            }
            if btn_response.clicked() {
                self.fullscreen_flag = !self.fullscreen_flag;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen_flag));
            }
        }
    }

    fn paint_frame_rate_text(&self, ui: &mut Ui, ctx: &Context, now: &Instant) {
        ui.horizontal(|ui| {
            let app_sec = (*now - self.app_start_instant).as_secs();
            if app_sec > 0 {
                let mut text_str = "帧率fps：".to_string();
                text_str.push_str((ctx.cumulative_frame_nr() / app_sec).to_string().as_str());
                let rich_text = egui::RichText::new(text_str)
                    .color(Color32::ORANGE)
                    .size(30.0);
                let fps_button = egui::Button::new(rich_text).frame(false);
                ui.add(fps_button);
            }
            let mut date_time_str = "日期时间：".to_string();
            if let Ok(formatter) =
                time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")
            {
                if let Ok(local_date_time) = time::OffsetDateTime::now_local() {
                    if let Ok(formatted_date_time_str) = local_date_time.format(&formatter) {
                        date_time_str.push_str(formatted_date_time_str.as_str());
                    }
                }
            }
            let rich_text = egui::RichText::new(date_time_str)
                .color(Color32::ORANGE)
                .size(30.0);
            let date_time_button = egui::Button::new(rich_text).frame(false);

            ui.add(date_time_button);
        });
    }
    fn check_play_is_at_endtail(&self) -> bool {
        if let Some(tiny_decoder) = &self.tiny_decoder {
            if let Ok(pts) = self.current_audio_frame_timestamp.read() {
                *pts / tiny_decoder.get_audio_time_base() as i64 + 1
                    >= tiny_decoder.get_end_audio_ts() / tiny_decoder.get_audio_time_base() as i64
            } else {
                false
            }
        } else {
            false
        }
    }

    fn audio_pts_to_current_play_pts(&self) {
        if let Some(audio_player) = &self.audio_player {
            if let Ok(pts) = audio_player.get_last_source_pts() {
                {
                    if let Ok(mut cur_pts) = self.current_audio_frame_timestamp.write() {
                        *cur_pts = pts;
                    }
                }
            }
        }
    }

    fn check_video_wait_for_audio(&self) -> bool {
        if let Some(tiny_decoder) = &self.tiny_decoder {
            if let Some(detail) = &self.current_frame_detail {
                if let Ok(timestamp) = self.current_audio_frame_timestamp.read() {
                    let v_time = detail.pts * 1000 / tiny_decoder.get_video_time_base() as i64;
                    let a_time = *timestamp * 1000 / tiny_decoder.get_audio_time_base() as i64;
                    if v_time - a_time > 1000 {
                        return true;
                    } else if v_time > a_time {
                        return false;
                    } else {
                        return true;
                    }
                }
            } else {
                return true;
            }
        }
        false
    }

    fn paint_open_network_content_box(&mut self, ui: &mut Ui, ctx: &Context, now: &Instant) {
        let openbox = egui::Checkbox::new(&mut self.show_content_window_flag, "network");
        let box_response = ui.add(openbox);
        if box_response.hovered() {
            self.control_ui_flag = true;
            self.last_show_control_ui_instant = now.clone();
        }

        if self.show_content_window_flag {
            let viewport_id = ViewportId::from_hash_of("content_window");
            ctx.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd_to(
                viewport_id,
                egui::ViewportCommand::Title("content_window".to_string()),
            );
            ctx.show_viewport_immediate(viewport_id, ViewportBuilder::default(), |ctx, _| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let online_btn = {
                        if self.async_ctx.get_online_flag() {
                            egui::Button::new("status:online")
                        } else {
                            egui::Button::new("status:offline").shortcut_text("click to try online")
                        }
                    };
                    let singleline = egui::TextEdit::singleline(&mut self.username_buf)
                        .hint_text("type in your temporary usename");
                    ui.add(singleline);
                    if ui.add(online_btn).clicked() {
                        self.async_ctx.login_server(self.username_buf.clone());
                        self.async_ctx.ws_connect_to_chat();
                    }
                    let msg_scroll_area = egui::ScrollArea::new(Vec2b::new(false, true));

                    msg_scroll_area
                        .id_salt("msg_scroll_area")
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.set_min_width(500.0);
                            for item in self.async_ctx.get_chat_msg_vec() {
                                let rich_text = egui::RichText::new(format!(
                                    "{}    :{}",
                                    item.get_name(),
                                    item.get_msg()
                                ))
                                .size(20.0);

                                ui.add(egui::Label::new(rich_text));
                            }
                        });
                    let input_scroll_area = egui::ScrollArea::new(Vec2b::new(false, true));
                    input_scroll_area
                        .id_salt("input_scroll_area")
                        .show(ui, |ui| {
                            ui.set_min_width(500.0);

                            ui.text_edit_multiline(&mut self.content_str);
                        });
                    if ui.button("send message").clicked() {
                        let detail = self.async_ctx.get_user_detail();
                        let mes = SocketChatMessage::new(
                            detail.id.clone(),
                            detail.username.clone(),
                            "server".to_string(),
                            self.content_str.to_string(),
                        );
                        self.async_ctx.ws_send_chat_msg(mes);
                        self.content_str.clear();
                    }
                    if ui.button("send req").clicked() {
                        if let Ok(url) = Url::from_str("http://bing.com") {
                            if let Ok(r) = self.async_ctx.req_external_url(Method::GET, url) {
                                self.content_str.clear();
                                self.content_str.push_str(r.as_str());
                            }
                        }
                    }
                });
            });
        }
    }
    fn reset_main_tex_to_bg(&mut self, ctx: &Context) {
        if let Some(v_tex) = &self.video_texture_id {
            ctx.tex_manager().write().set(
                *v_tex,
                ImageDelta::full(
                    ImageData::Color(Arc::new(ColorImage::from_rgba_unmultiplied(
                        [
                            self.bg_dyn_img.width() as usize,
                            self.bg_dyn_img.height() as usize,
                        ],
                        self.bg_dyn_img.as_bytes(),
                    ))),
                    TextureOptions::LINEAR,
                ),
            );
        }
    }
}
