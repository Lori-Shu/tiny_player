mod decode {
    use std::path::Path;

    use ffmpeg_next::{codec::Context, format, frame::Video, media::Type, software::scaling::Flags};

    


    struct TinyDecoder {}
    impl TinyDecoder {
        fn receive_and_process_decoded_frames(&self,decoder: &mut ffmpeg_next::decoder::Video,scale_ctx:&mut ffmpeg_next::software::scaling::Context)->Result<(),ffmpeg_next::Error>{
                let mut decoded = Video::empty();
                loop{
                if  decoder.receive_frame(&mut decoded).is_ok() {
                    let mut rgb_frame = Video::empty();
                    scale_ctx.run(&decoded, &mut rgb_frame)?;
                //     save_file(&rgb_frame, frame_index).unwrap();
                //     frame_index += 1;
                }else{
                        break;
                }
                }
                return Ok(());
        }
        fn format(&self) {
                ffmpeg_next::init().unwrap();
                let path = Path::new("hello.mp4");
                 let format_input=format::input(path).unwrap();
                 let video_stream = format_input.streams().best(Type::Video).unwrap();
                //  let stream_index = stream.index();
                 let decoder_ctx = Context::from_parameters(video_stream.parameters()).unwrap();
                let mut video_obj = decoder_ctx.decoder().video().unwrap();
                let mut scale_ctx = ffmpeg_next::software::scaler(format::Pixel::RGBA,Flags::BILINEAR,(video_obj.width(),video_obj.height()),(video_obj.width(),video_obj.height()) ).unwrap();
                let mut frame_index=0;
                
        }
    }
}
