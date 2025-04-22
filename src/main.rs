use audio_play::AudioPlayer;

mod decode;
mod audio_play;
fn main() {
    println!("Hello, world!");
    let mut tiny_decoder = decode::TinyDecoder::new();
    let mut audio_player=audio_play::AudioPlayer::new();
    audio_player.init_device();
    tiny_decoder.set_file_path_and_init_par(std::path::Path::new("D:/Downloads/全职高手第二季12.mp4"));
    tiny_decoder.start_process_threads();
    loop{
    if let Some(frame)=tiny_decoder.get_one_audio_play_frame() {
        audio_player.play_raw_data_from_audio_frame(frame);
    }
}
    let mut str=String::new();
    std::io::stdin().read_line(&mut str).unwrap();
}
