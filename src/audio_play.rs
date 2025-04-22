      pub struct AudioPlayer{
                sink:Option<rodio::Sink>,
                stream:Option<rodio::OutputStream>,
                handle:Option<rodio::OutputStreamHandle>
        }
        impl AudioPlayer{
               pub fn new()->Self{
                        return Self {  
                                stream:None,
                                handle:None,
                                sink:None
                        };
                }
                pub fn init_device(&mut self){
                        let (stream,handle) = rodio::OutputStream::try_default().unwrap();
                        let sink=rodio::Sink::try_new(&handle).unwrap();
                        self.sink=Some(sink);
                        self.handle=Some(handle);
                        self.stream=Some(stream);
                }
               pub fn play_raw_data_from_audio_frame(&self,audio_frame:ffmpeg_next::frame::Audio){
                        let audio_data:&[i16]=bytemuck::cast_slice(audio_frame.data(0));
                        let source=rodio::buffer::SamplesBuffer::new(audio_frame.channels(), audio_frame.rate(),audio_data);
                        
                        let sink = self.sink.as_ref().unwrap();
                        sink.append(source);
                        println!("sinkpos==>{}",sink.get_pos().as_millis());
                        // sink.play();
                }
        }
