use std::{
    ffi::CString,
    ptr::{null, null_mut},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use ffmpeg_the_third::{
    ffi::{
        AVFormatContext, AVIO_FLAG_WRITE, AVMediaType, av_interleaved_write_frame,
        av_packet_rescale_ts, av_write_trailer, avcodec_parameters_copy,
        avformat_alloc_output_context2, avformat_free_context, avformat_new_stream,
        avformat_write_header, avio_closep, avio_open2,
    },
    packet::Mut,
};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use image::EncodableLayout;
use log::{error, warn};
use reqwest::{Client, Method, Url};
use reqwest_websocket::{Bytes, Message, RequestBuilderExt, WebSocket};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Runtime, sync::RwLock, time::sleep};

use crate::{PlayerError, PlayerResult};

enum ServerApi {
    Login,
    ChatMsg,
    ShareVideoSock,
}
impl ServerApi {
    pub fn get_url_from_server_api(self) -> PlayerResult<Url> {
        match self {
            Self::Login => {
                if let Ok(url) = Url::from_str("http://127.0.0.1:8536/user/guest_login") {
                    Ok(url)
                } else {
                    Err(PlayerError::Internal("Login err".to_string()))
                }
            }
            Self::ChatMsg => {
                if let Ok(url) = Url::from_str("ws://127.0.0.1:8536/player/chat") {
                    Ok(url)
                } else {
                    Err(PlayerError::Internal("ChatMsg err".to_string()))
                }
            }
            Self::ShareVideoSock => {
                if let Ok(url) = Url::from_str("ws://127.0.0.1:8536/player/share_sock") {
                    Ok(url)
                } else {
                    Err(PlayerError::Internal("ShareVideoSock err".to_string()))
                }
            }
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoDes {
    pub name: String,
    pub path: String,
    pub user_id: String,
}
pub struct AsyncContext {
    async_runtime: Arc<Runtime>,
    request_manager: Arc<RequestManager>,
    websocket_manager: Arc<WebSocketManager>,
    online_flag: bool,
    user_detail: Arc<RwLock<UserDetail>>,
    chat_msg_vec: Arc<RwLock<Vec<SocketMessage>>>,
    ui_msg_vec: Vec<SocketMessage>,
    online_videos: Vec<VideoDes>,
}
impl AsyncContext {
    pub fn new() -> PlayerResult<Self> {
        if let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            let rt = Arc::new(runtime);
            let req_man = Arc::new(RequestManager::new());
            let req_client = req_man.get_client().clone();
            let s = Self {
                async_runtime: rt,
                request_manager: req_man,
                websocket_manager: Arc::new(WebSocketManager::new(req_client)),
                online_flag: false,
                user_detail: Arc::new(RwLock::new(UserDetail {
                    id: String::new(),
                    username: String::new(),
                    role: String::new(),
                })),
                chat_msg_vec: Arc::new(RwLock::new(Vec::new())),
                ui_msg_vec: vec![],
                online_videos: vec![],
            };
            Ok(s)
        } else {
            Err(PlayerError::Internal(
                "build async runtime error".to_string(),
            ))
        }
    }
    pub fn req_external_url(&self, me: Method, url: Url) -> Result<String, String> {
        let req_manager = self.request_manager.clone();
        self.async_runtime
            .block_on(async move { req_manager.execute_req(me, url).await })
    }
    pub fn ws_connect_to_chat(&self) {
        let manager = self.websocket_manager.clone();
        let ve = self.chat_msg_vec.clone();
        let user = self.user_detail.clone();
        self.async_runtime.block_on(async move {
            manager.connect_chat(user, ve).await;
        });
    }
    pub fn get_online_flag(&self) -> bool {
        self.online_flag
    }
    pub fn login_server(&mut self, username: String, format_duration: Arc<RwLock<i64>>) {
        let request_manager = self.request_manager.clone();
        let user_detail = self.async_runtime.block_on(async move {
            if let Ok(user_detail) = request_manager.login(username).await {
                Ok(user_detail)
            } else {
                Err(PlayerError::Internal("login failed".to_string()))
            }
        });
        self.online_flag = true;
        if let Ok(user_detail) = user_detail {
            self.user_detail = user_detail;
        }
        self.ws_connect_to_chat();
        self.ws_connect_share(format_duration);
    }
    pub fn get_chat_msg_vec(&mut self) -> &Vec<SocketMessage> {
        let v = self.chat_msg_vec.clone();
        let ui_msg_v_len = self.ui_msg_vec.len();
        if let Some(new_v) = self.async_runtime.block_on(async move {
            let chat_v = v.write().await;
            if ui_msg_v_len != chat_v.len() {
                Some((*chat_v).clone())
            } else {
                None
            }
        }) {
            self.ui_msg_vec = new_v;
        }
        &self.ui_msg_vec
    }
    pub fn ws_send_chat_msg(&self, message: SocketMessage) {
        let manager = self.websocket_manager.clone();
        self.async_runtime.block_on(async move {
            manager.send_msg(message).await;
        });
    }
    pub fn get_user_detail(&self) -> UserDetail {
        (*self.async_runtime.block_on(self.user_detail.read())).clone()
    }
    pub fn exec_normal_task<Fu, Ot>(&self, f: Fu) -> Ot
    where
        Fu: Future<Output = Ot>,
    {
        self.async_runtime.block_on(f)
    }

    pub fn ws_connect_share(&self, end_ts: Arc<RwLock<i64>>) {
        let ws_man = self.websocket_manager.clone();
        self.async_runtime.block_on(async move {
            ws_man.connect_share(end_ts).await;
        });
    }
    pub fn share_video(&mut self, share_target: Vec<VideoDes>) {
        let sock_man = self.websocket_manager.clone();
        self.async_runtime.block_on(async move {
            sock_man.publish_video(share_target).await;
        });
    }
    pub fn watch_shared_video(&mut self, target: VideoDes) {
        let sock_man = self.websocket_manager.clone();
        self.async_runtime.block_on(async move {
            sock_man.req_watch_video(target).await;
        });
    }
    pub fn get_online_videos(&mut self) -> &Vec<VideoDes> {
        let v = self
            .async_runtime
            .block_on(self.websocket_manager.video_des_vec.write());
        if v.len() != self.online_videos.len() {
            self.online_videos = v.clone();
        }
        &self.online_videos
    }
    pub fn get_runtime(&self) -> Arc<Runtime> {
        self.async_runtime.clone()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserDetail {
    pub id: String,
    pub username: String,
    pub role: String,
}
pub struct RequestManager {
    client: Arc<Client>,
    user_detail: Arc<RwLock<UserDetail>>,
}
impl RequestManager {
    pub fn new() -> Self {
        let s = Self {
            client: Arc::new(Client::default()),
            user_detail: Arc::new(RwLock::new(UserDetail {
                id: String::new(),
                username: String::new(),
                role: String::new(),
            })),
        };
        s
    }
    pub fn get_client(&self) -> &Arc<Client> {
        &self.client
    }

    async fn execute_req(&self, method: Method, url: Url) -> Result<String, String> {
        if let Ok(response) = self.client.request(method, url).send().await {
            if let Ok(bytes) = response.bytes().await {
                if let Ok(str) = String::from_utf8(bytes.to_vec()) {
                    Ok(str)
                } else {
                    Err("bytes to string err".to_string())
                }
            } else {
                Err("extract response bytes err".to_string())
            }
        } else {
            Err("req err".to_string())
        }
    }
    async fn login(&self, username: String) -> PlayerResult<Arc<RwLock<UserDetail>>> {
        if let Ok(url) = ServerApi::get_url_from_server_api(ServerApi::Login) {
            if let Ok(response) = self.client.post(url).body(username).send().await {
                if let Ok(bytes) = response.bytes().await {
                    if let Ok(str) = String::from_utf8(bytes.to_vec()) {
                        if let Ok(user_detail) = serde_json::from_str::<UserDetail>(&str) {
                            warn!("receive detail{:?}", user_detail);
                            *self.user_detail.write().await = user_detail;
                            return Ok(self.user_detail.clone());
                        } else {
                            return Err(PlayerError::Internal("json parse err".to_string()));
                        }
                    } else {
                        return Err(PlayerError::Internal("bytes to string err".to_string()));
                    }
                } else {
                    return Err(PlayerError::Internal(
                        "extract response bytes err".to_string(),
                    ));
                }
            } else {
                return Err(PlayerError::Internal("req err".to_string()));
            }
        }
        Err(PlayerError::Internal("req err".to_string()))
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SocketMessage {
    sender_id: String,
    sender_name: String,
    receiver_type: String,
    receiver_id: String,
    msg_type: String,
    msg: Vec<u8>,
}
impl SocketMessage {
    pub fn new(
        sender_id: String,
        sender_name: String,
        receiver_type: String,
        receiver_id: String,
        msg_type: String,
        msg: Vec<u8>,
    ) -> Self {
        Self {
            sender_id,
            sender_name,
            receiver_type,
            receiver_id,
            msg_type,
            msg,
        }
    }
    pub fn get_name(&self) -> &String {
        &self.sender_name
    }
    pub fn get_msg(&self) -> &Vec<u8> {
        &self.msg
    }
    pub fn get_msg_type(&self) -> &String {
        &self.msg_type
    }
}

struct WebSocketManager {
    client: Arc<Client>,
    user: Arc<RwLock<UserDetail>>,
    chat_socket_sink: Arc<RwLock<Option<SplitSink<WebSocket, Message>>>>,
    chat_socket_stream: Arc<RwLock<Option<SplitStream<WebSocket>>>>,
    share_socket_sink: Arc<RwLock<Option<SplitSink<WebSocket, Message>>>>,
    share_socket_stream: Arc<RwLock<Option<SplitStream<WebSocket>>>>,
    video_des_vec: Arc<RwLock<Vec<VideoDes>>>,
}

impl WebSocketManager {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client: client,
            user: Arc::new(RwLock::new(UserDetail {
                id: String::new(),
                username: String::new(),
                role: String::new(),
            })),
            chat_socket_sink: Arc::new(RwLock::new(None)),
            chat_socket_stream: Arc::new(RwLock::new(None)),
            share_socket_sink: Arc::new(RwLock::new(None)),
            share_socket_stream: Arc::new(RwLock::new(None)),
            video_des_vec: Arc::new(RwLock::new(vec![])),
        }
    }
    pub async fn connect_chat(
        &self,
        user_detail: Arc<RwLock<UserDetail>>,
        msg_vec: Arc<RwLock<Vec<SocketMessage>>>,
    ) {
        if let Ok(url) = ServerApi::get_url_from_server_api(ServerApi::ChatMsg) {
            warn!("url to connect websocket{}", url.as_str());
            if let Ok(res) = self.client.get(url).upgrade().send().await {
                let web_socket = res.into_websocket().await;
                if let Ok(mut ws) = web_socket {
                    {
                        let user = user_detail.read().await;
                        let msg_str = format!("user {:?} is now online", user.username);
                        let msg = SocketMessage::new(
                            user.id.clone(),
                            user.username.clone(),
                            "server".to_string(),
                            "server".to_string(),
                            "chat init".to_string(),
                            msg_str.as_bytes().to_vec(),
                        );
                        if let Ok(json_msg) = serde_json::to_string(&msg) {
                            if ws
                                .send(reqwest_websocket::Message::Binary(Bytes::from(
                                    json_msg.as_bytes().to_vec(),
                                )))
                                .await
                                .is_ok()
                            {
                                warn!("连接到chat服务");
                                {
                                    let (sink, stream) = ws.split();
                                    *self.chat_socket_sink.write().await = Some(sink);
                                    *self.chat_socket_stream.write().await = Some(stream);
                                }
                                *self.user.write().await = (*user).clone();
                            }
                        }
                    }
                    let stream = self.chat_socket_stream.clone();
                    tokio::spawn(Self::watch_server_chat_msg(stream, msg_vec));
                } else if let Err(e) = web_socket {
                    error!("connect server err{:?}", e);
                }
            } else {
                warn!("into websocket res err");
            }
        }
    }
    async fn watch_server_chat_msg(
        chat_socket_stream: Arc<RwLock<Option<SplitStream<WebSocket>>>>,
        msg_vec: Arc<RwLock<Vec<SocketMessage>>>,
    ) {
        loop {
            sleep(Duration::from_millis(10)).await;
            {
                let mut sock = chat_socket_stream.write().await;
                if let Some(sock) = &mut *sock {
                    if let Some(Ok(msg)) = sock.next().await {
                        if let reqwest_websocket::Message::Binary(bytes) = msg {
                            warn!("收到信息{:?}", bytes);
                            if let Ok(msg) =
                                serde_json::from_slice::<'_, SocketMessage>(bytes.as_bytes())
                            {
                                let mut v = msg_vec.write().await;
                                v.push(msg);
                            } else {
                                todo!();
                            }
                        } else {
                            todo!();
                        }
                    } else {
                        todo!();
                    }
                }
            }
        }
    }
    async fn send_msg(&self, message: SocketMessage) {
        warn!("before write ws");
        let mut ws = self.chat_socket_sink.write().await;
        if let Some(socket) = &mut *ws {
            if let Ok(bin) = serde_json::to_vec(&message) {
                warn!("before send message");
                let send_res = socket
                    .send(reqwest_websocket::Message::Binary(Bytes::from_owner(bin)))
                    .await;
                if send_res.is_ok() {
                    warn!("send msg success");
                } else if let Err(e) = send_res {
                    warn!("send bin err{:?}", e);
                }
            } else {
                warn!("deserialize err");
            }
        } else {
            warn!("you are offline");
        }
    }
    async fn connect_share(&self, format_duration: Arc<RwLock<i64>>) {
        if let Ok(url) = ServerApi::get_url_from_server_api(ServerApi::ShareVideoSock) {
            if let Ok(res) = self.client.get(url).upgrade().send().await {
                if let Ok(socket) = res.into_websocket().await {
                    let (mut sink, stream) = socket.split();
                    {
                        let user = self.user.read().await;
                        warn!("connect share user id{:?}", user.id);
                        let socket_message = SocketMessage::new(
                            user.id.clone(),
                            user.username.clone(),
                            "server".to_string(),
                            "server".to_string(),
                            "share init".to_string(),
                            "".to_string().as_bytes().to_vec(),
                        );
                        if let Ok(str) = serde_json::to_string(&socket_message) {
                            if sink
                                .send(reqwest_websocket::Message::Binary(Bytes::from(
                                    str.as_bytes().to_vec(),
                                )))
                                .await
                                .is_ok()
                            {
                                warn!("send msg success");
                            }
                        }
                        {
                            *self.share_socket_sink.write().await = Some(sink);
                            *self.share_socket_stream.write().await = Some(stream);
                        }
                    }
                    let share_socket_sink = self.share_socket_sink.clone();
                    let share_socket_stream = self.share_socket_stream.clone();
                    let video_des_vec = self.video_des_vec.clone();
                    tokio::spawn(async move {
                        loop {
                            sleep(Duration::from_millis(10)).await;
                            let mut stream = share_socket_stream.write().await;
                            if let Some(share_stream) = &mut *stream {
                                if let Some(Ok(message)) = share_stream.next().await {
                                    if let Message::Binary(bts) = &message {
                                        if let Ok(msg) =
                                            serde_json::from_slice::<SocketMessage>(bts.as_bytes())
                                        {
                                            if msg.get_msg_type().eq("video des") {
                                                warn!("receive video des!!!");
                                                if let Ok(des) = serde_json::from_slice::<VideoDes>(
                                                    &msg.get_msg(),
                                                ) {
                                                    let mut des_v = video_des_vec.write().await;
                                                    des_v.push(des);
                                                }
                                            } else if msg.get_msg_type().eq("req video command") {
                                                warn!("receve push video command!!!");
                                                let sink = share_socket_sink.clone();
                                                tokio::spawn(async move {
                                                    Self::push_video_stream(msg, sink).await;
                                                });
                                            } else if msg
                                                .get_msg_type()
                                                .eq("duration of video to play")
                                            {
                                                if let Ok(sli) =
                                                    (*msg.get_msg().as_bytes()).try_into()
                                                {
                                                    *format_duration.write().await =
                                                        i64::from_le_bytes(sli);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            }
        }
    }
    async fn publish_video(&self, target: Vec<VideoDes>) {
        let mut share_socket = self.share_socket_sink.write().await;
        if let Some(sock) = &mut (*share_socket) {
            let user = self.user.read().await;
            for item in target {
                if let Ok(des) = serde_json::to_string(&item) {
                    let socket_message = SocketMessage::new(
                        user.id.clone(),
                        user.username.clone(),
                        "server".to_string(),
                        String::new(),
                        "video des".to_string(),
                        des.as_bytes().to_vec(),
                    );
                    if let Ok(msg) = serde_json::to_string(&socket_message) {
                        if sock
                            .send(Message::Binary(Bytes::from(msg.as_bytes().to_vec())))
                            .await
                            .is_ok()
                        {
                        } else {
                            todo!();
                        }
                    }
                }
            }
        }
    }
    async fn push_video_stream(
        socket_msg: SocketMessage,
        sink: Arc<RwLock<Option<SplitSink<WebSocket, Message>>>>,
    ) {
        if let Ok(des) = serde_json::from_slice::<VideoDes>(socket_msg.get_msg().clone().as_bytes())
        {
            warn!("req path{}", des.path);
            if let Ok(mut input) = ffmpeg_the_third::format::input(des.path) {
                let mut sink = sink.write().await;
                let vec = input.duration().to_le_bytes().to_vec();
                let so_msg = SocketMessage::new(
                    socket_msg.receiver_id,
                    "".to_string(),
                    "user".to_string(),
                    socket_msg.sender_id,
                    "duration of video to play".to_string(),
                    vec,
                );
                if let Ok(v) = serde_json::to_vec(&so_msg) {
                    if let Some(sink) = &mut *sink {
                        if sink.send(Message::Binary(Bytes::from(v))).await.is_ok() {
                            warn!("send message success");
                        }
                    }
                }
                {
                    unsafe {
                        let mut out_ctx: *mut AVFormatContext = null_mut();
                        if let Ok(fmt) = CString::new("mp4") {
                            if let Ok(s) = CString::new("stream") {
                                // 格式要手动指定
                                let mut res = avformat_alloc_output_context2(
                                    (&mut out_ctx) as *mut *mut AVFormatContext,
                                    null(),
                                    fmt.as_ptr(),
                                    s.as_ptr(),
                                );
                                if res != 0 {
                                    todo!();
                                }
                                // 复制所有流（不重新编码）
                                for (stream_index, istream) in input.streams().enumerate() {
                                    let ostream = avformat_new_stream(out_ctx, null());
                                    if (*istream.parameters().as_ptr()).codec_type
                                        == AVMediaType::AVMEDIA_TYPE_VIDEO
                                    {
                                        warn!("video tstream");
                                    }
                                    if ostream.is_null() {
                                        todo!();
                                    }
                                    {
                                        let pars = istream.parameters();
                                        res = avcodec_parameters_copy(
                                            (*ostream).codecpar,
                                            pars.as_ptr(),
                                        );
                                        if res != 0 {
                                            todo!();
                                        }
                                        println!("Added stream {} ->", stream_index);
                                    }
                                }

                                if let Ok(s) = CString::new("tcp://127.0.0.1:18859") {
                                    // 打开输出上下文
                                    res = avio_open2(
                                        &mut (*out_ctx).pb,
                                        s.as_ptr(),
                                        AVIO_FLAG_WRITE,
                                        null_mut(),
                                        null_mut(),
                                    );
                                    if res != 0 {
                                        todo!();
                                    }
                                    res = avformat_write_header(out_ctx, null_mut());
                                    if res != 0 {
                                        todo!();
                                    }
                                    // 循环读取并写入 packet
                                    for packet in input.packets() {
                                        if let Ok(mut pkt) = packet {
                                            let in_stream = pkt.0;
                                            // 先取到 AVStream* 指针
                                            let out_stream_ptr = *(*out_ctx)
                                                .streams
                                                .offset(in_stream.index() as isize);
                                            let out_stream = &*out_stream_ptr; // &AVStream, 安全访问字段
                                            av_packet_rescale_ts(
                                                pkt.1.as_mut_ptr(),
                                                (*in_stream.as_ptr()).time_base,
                                                (*out_stream).time_base,
                                            );
                                            (*pkt.1.as_mut_ptr()).stream_index =
                                                (*out_stream).index;
                                            // (*pkt.1.as_mut_ptr()).stream_index = ;
                                            res = av_interleaved_write_frame(
                                                out_ctx,
                                                pkt.1.as_mut_ptr(),
                                            );
                                            if res != 0 {
                                                warn!("write err{}", res);
                                                todo!();
                                            }
                                        }
                                    }
                                    res = av_write_trailer(out_ctx);
                                    if res != 0 {
                                        todo!();
                                    }
                                    res = avio_closep(&mut (*out_ctx).pb);
                                    if res != 0 {
                                        todo!();
                                    }
                                    avformat_free_context(out_ctx);
                                    warn!("push video stream completed");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn req_watch_video(&self, video: VideoDes) {
        let mut socket = self.share_socket_sink.write().await;
        if let Some(sock) = &mut *socket {
            if let Ok(des) = serde_json::to_string(&video) {
                let user = self.user.read().await;
                let socket_message = SocketMessage::new(
                    user.id.clone(),
                    user.username.clone(),
                    "user".to_string(),
                    video.user_id.clone(),
                    "req video command".to_string(),
                    des.as_bytes().to_vec(),
                );
                if let Ok(msg) = serde_json::to_string(&socket_message) {
                    if sock
                        .send(Message::Binary(Bytes::from(msg.as_bytes().to_vec())))
                        .await
                        .is_ok()
                    {}
                }
            }
        }
    }
}
#[cfg(test)]
mod test {

    fn compute_medium(arr: &[i32], i: &usize, j: &usize, k: &usize) -> usize {
        let a = arr[*i];
        let b = arr[*j];
        let c = arr[*k];
        if a > b {
            if b > c {
                *j
            } else if a > c {
                *k
            } else {
                *i
            }
        } else if a > c {
            *i
        } else if b > c {
            *k
        } else {
            *j
        }
    }
    // 手写采用中值pivot,三路分区和双向扫描的快速排序
    // 快速排序里的双索引扫描双索引并不是第一移动元素，
    // 存在第三个作为第一移动的索引
    fn quick_sort(arr: &mut [i32]) {
        if arr.len() < 2 {
            return;
        }
        let first = 0;
        let mid = arr.len() / 2;
        let last = arr.len() - 1;
        let pivot = compute_medium(arr, &first, &mid, &last);
        println!("pivot{:?}", arr[pivot]);
        arr.swap(last, pivot);
        let (mut left_index, mut idx, mut right_index) = (0, 0, last - 1);
        loop {
            if idx > right_index {
                break;
            }
            if arr[idx] > arr[last] {
                arr.swap(idx, right_index);
                if right_index == 0 {
                    break;
                }
                right_index -= 1;
            } else if arr[idx] < arr[last] {
                arr.swap(idx, left_index);
                left_index += 1;
                if idx < left_index {
                    idx += 1;
                }
            } else {
                idx += 1;
            }
        }
        arr.swap(idx, last);

        let (lt, egt) = arr.split_at_mut(left_index);

        let (e, gt) = egt.split_at_mut(idx + 1 - lt.len());
        println!("eq{:?}", e);
        quick_sort(lt);
        quick_sort(gt);
    }
    pub fn shell_sort<T: Ord>(arr: &mut [T]) {
        let n = arr.len();

        // 生成 Tokuda 序列
        let mut gaps = Vec::new();
        let mut k = 1;
        loop {
            let gap = ((9_i64.pow(k) - 4_i64.pow(k)) / (5 * 4_i64.pow(k - 1))) as usize;
            if gap > n {
                break;
            }
            gaps.push(gap);
            k += 1;
        }
        gaps.reverse(); // 从大到小使用

        // 希尔排序核心
        for &gap in &gaps {
            for i in gap..n {
                let mut j = i;

                loop {
                    if j >= n {
                        break;
                    }
                    if arr[j - gap] > arr[j] {
                        arr.swap(j - gap, j);
                    }
                    j += 1;
                }
            }
        }
    }
    #[test]
    fn test_quick_sort() {
        let mut arr = [
            1236985745, 213, 213, -3069, 2000, 213, 569, 265, 231, 444, 578, 032, -136, 123, 589,
            987, 625, 301, 203, 10, 9999,
        ];
        quick_sort(&mut arr);
        println!("{:?}", arr);
    }
    #[test]
    fn test_shell_sort() {
        let mut arr = [
            1236985745, 213, 213, -3069, 2000, 213, 569, 265, 231, 444, 578, 032, -136, 123, 589,
            987, 625, 301, 203, 10, 9999,
        ];
        shell_sort(&mut arr);
        println!("{:?}", arr);
    }
}
