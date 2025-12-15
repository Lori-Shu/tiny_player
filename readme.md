# Basic
A video player,aims to be light and simple;
core feature based on project
[ffmpeg](https://github.com/FFmpeg/FFmpeg)
[ffmpeg-the-third](https://github.com/shssoichiro/ffmpeg-the-third)
[egui](https://github.com/emilk/egui)
# Why should you use tiny_player
1. tiny_player is small, its storage will never cross 1 G, 
   hopefully
2. tiny_player is fast, with pure rust language, modern ui 
   framework [egui](https://github.com/emilk/egui) and maybe the 
   fastest decoder by [ffmpeg](https://github.com/FFmpeg/FFmpeg) 
   and even more, render by the 
   [wgpu](https://github.com/gfx-rs/wgpu) vulkan renderer
3. tiny_player is opensource, you can modify code and build your 
   own app under LICENSE
4. AI subtitle function is ready for trying
# Usage
1. currently only support windows
2. run the tiny_installer.exe
3. run the tiny_player.exe on desktop
4. click the file button
5. select a video file ,normally .mp4 or .mkv
6. click open 
7. click the play button
8. control the video by the control widges
# Tips
1. if you use a laptop and player runs at a very low frame 
rate, turn off "BatteryBoost" in nvidia driver which is to save 
energy.