use std::{str::FromStr, sync::Arc, time::Duration};

use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use log::{error, warn};
use reqwest::{Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use tokio::{runtime::Runtime, sync::RwLock, time::sleep};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{ClientRequestBuilder, Message, client::IntoClientRequest, http::Uri},
};
enum ServerApi {
    Login,
    ChatMsg,
}
impl ServerApi {
    pub fn get_url_from_server_api(self) -> Url {
        match self {
            Self::Login => {
                if let Ok(url) = Url::from_str("http://127.0.0.1:8536/user/guest_login") {
                    url
                } else {
                    panic!()
                }
            }
            Self::ChatMsg => {
                if let Ok(url) = Url::from_str("ws://127.0.0.1:8536/player/chat") {
                    url
                } else {
                    panic!()
                }
            }
        }
    }
}
pub struct AsyncContext {
    async_runtime: Arc<Runtime>,
    request_manager: Arc<RequestManager>,
    websocket_manager: Arc<WebSocketManager>,
    online_flag: bool,
    user_detail: Option<UserDetail>,
    chat_msg_vec: Arc<RwLock<Vec<SocketChatMessage>>>,
    ui_msg_vec: Vec<SocketChatMessage>,
}
impl AsyncContext {
    pub fn new() -> Self {
        if let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            let rt = Arc::new(runtime);
            let req_man = Arc::new(RequestManager::new());
            let rt_cloned = rt.clone();
            let s = Self {
                async_runtime: rt_cloned,
                request_manager: req_man,
                websocket_manager: Arc::new(WebSocketManager::new()),
                online_flag: false,
                user_detail: None,
                chat_msg_vec: Arc::new(RwLock::new(Vec::new())),
                ui_msg_vec: vec![],
            };
            s
        } else {
            panic!();
        }
    }
    pub fn req_external_url(&self, me: Method, url: Url) -> Result<String, String> {
        let req_manager = self.request_manager.clone();
        self.async_runtime
            .block_on(async move { req_manager.execute_req(me, url).await })
    }
    pub fn ws_connect_to_chat(&self) {
        if let Some(u) = &self.user_detail {
            let manager = self.websocket_manager.clone();
            let ve = self.chat_msg_vec.clone();
            let u = u.clone();
            self.async_runtime.spawn(async move {
                manager.connect_chat(u, ve).await;
            });
        } else {
            panic!()
        }
    }
    pub fn get_online_flag(&self) -> bool {
        self.online_flag
    }
    pub fn login_server(&mut self,username:String) {
        let request_manager = self.request_manager.clone();
        self.user_detail = Some(self.async_runtime.block_on(async move {
            if let Ok(user_detail) = request_manager.login(username).await {
                user_detail
            } else {
                panic!("login failed");
            }
        }));
        self.online_flag = true;
    }
    pub fn get_chat_msg_vec(&mut self) -> &Vec<SocketChatMessage> {
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
    pub fn ws_send_chat_msg(&self, message: SocketChatMessage) {
        let manager = self.websocket_manager.clone();
        self.async_runtime.block_on(async move {
            manager.send_msg(message).await;
        });
    }
    pub fn get_user_detail(&self) -> &UserDetail {
        if let Some(u) = &self.user_detail {
            u
        } else {
            panic!("you are offline!");
        }
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
}
impl RequestManager {
    pub fn new() -> Self {
        let s = Self {
            client: Arc::new(Client::new()),
        };
        s
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
    async fn login(&self,username:String) -> Result<UserDetail, String> {
        if let Ok(response) = self
            .client
            .post(ServerApi::get_url_from_server_api(ServerApi::Login))
            .body(username)
            .send()
            .await
        {
            if let Ok(bytes) = response.bytes().await {
                if let Ok(str) = String::from_utf8(bytes.to_vec()) {
                    if let Ok(user_detail) = serde_json::from_str(&str) {
                        Ok(user_detail)
                    } else {
                        Err("json parse err".to_string())
                    }
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
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SocketChatMessage {
    sender_id: String,
    sender_name: String,
    receiver_type: String,
    msg: String,
}
impl SocketChatMessage {
    pub fn new(sender_id: String, sender_name: String, receiver_type: String, msg: String) -> Self {
        Self {
            sender_id,
            sender_name,
            receiver_type,
            msg,
        }
    }
    pub fn get_name(&self) -> &String {
        &self.sender_name
    }
    pub fn get_msg(&self) -> &String {
        &self.msg
    }
}
struct WebSocketManager {
    chat_socket_sink: Arc<
        RwLock<Option<SplitSink<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>, Message>>>,
    >,
    chat_socket_stream:
        Arc<RwLock<Option<SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>>>>,
}
impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            chat_socket_sink: Arc::new(RwLock::new(None)),
            chat_socket_stream: Arc::new(RwLock::new(None)),
        }
    }
    pub async fn connect_chat(
        &self,
        user_detail: UserDetail,
        msg_vec: Arc<RwLock<Vec<SocketChatMessage>>>,
    ) {
        let url = ServerApi::get_url_from_server_api(ServerApi::ChatMsg);
        if let Ok(uri) = Uri::from_str(url.as_str()) {
            warn!("connect uri{:?}", uri);
            if let Ok(client_request) = ClientRequestBuilder::new(uri).into_client_request() {
                if let Ok((ws, res)) = connect_async(client_request).await {
                    if res.status() == StatusCode::SWITCHING_PROTOCOLS {
                        let (mut sink, stream) = ws.split();
                        let msg_str = format!("user {:?} is now online", user_detail.username);
                        let msg = SocketChatMessage::new(
                            user_detail.id.clone(),
                            user_detail.username.clone(),
                            "server".to_string(),
                            msg_str,
                        );
                        if let Ok(json_msg) = serde_json::to_string(&msg) {
                            if let Ok(_) = sink
                                .send(tokio_tungstenite::tungstenite::Message::binary(json_msg))
                                .await
                            {
                                warn!("连接到chat服务");
                                {
                                    let mut chat_sink = self.chat_socket_sink.write().await;
                                    *chat_sink = Some(sink);
                                    let mut chat_stream = self.chat_socket_stream.write().await;
                                    *chat_stream = Some(stream);
                                }
                                self.watch_server_chat_msg(msg_vec).await;
                            }
                        } else {
                            error!("json serialize err");
                        }
                    } else {
                        warn!("statuscode{:?}", res.status());
                    }
                } else {
                    panic!()
                }
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }
    async fn watch_server_chat_msg(&self, msg_vec: Arc<RwLock<Vec<SocketChatMessage>>>) {
        loop {
            sleep(Duration::from_millis(10)).await;
            {
                let mut sock = self.chat_socket_stream.write().await;
                if let Some(sock) = &mut *sock {
                    if let Some(Ok(msg)) = sock.next().await {
                        let bytes = msg.into_data();
                        if let Ok(msg) = serde_json::from_slice::<'_, SocketChatMessage>(
                            bytes.to_vec().as_slice(),
                        ) {
                            let mut v = msg_vec.write().await;
                            v.push(msg);
                        }
                    }
                }
            }
        }
    }
    async fn send_msg(&self, message: SocketChatMessage) {
        warn!("before write sink");
        let mut sink = self.chat_socket_sink.write().await;
        if let Some(socket) = &mut *sink {
            if let Ok(bin) = serde_json::to_vec(&message) {
                warn!("before send message");
                let send_res=socket.send(Message::binary(bin)).await;
                if let Ok(_) = send_res {
                    warn!("send msg success");
                } else if let Err(e) = send_res{
                    panic!("send bin err{:?}",e);
                }
            } else {
                panic!("deserialize err");
            }
        } else {
            panic!("you are offline");
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
