use eframe::egui;
use egui::{ColorImage, TextureHandle};
use image::{DynamicImage, GenericImageView};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};

use std::sync::Mutex as StdMutex;

use crate::enums::app_type::AppType;
use crate::enums::connection_status::{self, ConnectionStatus};
use crate::enums::message::{
    construct_connection_message, construct_image_message, construct_image_message_generic,
    construct_text_message, construct_text_message_generic, CombinedMessage, LogMessage, Message,
    TextMessage,
};
use crate::network::network_utils::handle_connection;

type ChatHistory = Arc<StdMutex<Vec<Message>>>;
type Log = Arc<StdMutex<Vec<LogMessage>>>;

type AsyncSender = Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>;
type AsyncReceiver = Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>;

type ProfileType = Arc<StdMutex<Profile>>;

#[derive(Clone)]
pub struct Profile {
    pub image: Option<ColorImage>,
    pub texture: Option<TextureHandle>,
    sent: bool,
}

pub fn color_image_to_bytes(color_image: &egui::ColorImage) -> Vec<u8> {
    // Extract dimensions and pixel data
    let (width, height) = (color_image.size[0], color_image.size[1]);
    let pixels = &color_image.pixels;

    // Create a vector to hold the raw byte data
    let mut bytes = Vec::with_capacity((width * height * 4) as usize); // 4 bytes per pixel (RGBA)

    // Convert the `ColorImage` pixels to a byte vector
    for pixel in pixels {
        bytes.push(pixel[0]); // Red
        bytes.push(pixel[1]); // Green
        bytes.push(pixel[2]); // Blue
        bytes.push(pixel[3]); // Alpha
    }

    bytes
}

impl Profile {
    pub fn new(filepath: &str, size_reduction: f32) -> Self {
        let mut result = Self::default();
        if let Ok(image) = image::open(filepath) {
            let width = image.width();
            let height = image.height();

            let new_width = (width as f32 * size_reduction) as u32;
            let new_height = (height as f32 * size_reduction) as u32;

            let image = image.resize(new_width, new_height, image::imageops::FilterType::Nearest);
            println!(
                "Resized image from {}x{} to {}x{}",
                width, height, new_width, new_height,
            );

            let dim = image.dimensions();
            let size = [dim.0 as usize, dim.1 as usize];
            let color_image = ColorImage::from_rgba_unmultiplied(size, &image.to_rgba8().to_vec());
            result.image = Some(color_image.clone());
            println!(
                "Size of bytes, {:?}",
                color_image_to_bytes(&color_image).len(),
            );
            result
        } else {
            result
        }
    }

    pub fn from_bytes(bytes: Vec<u8>, width: u16, height: u16) -> Self {
        let size = [width as usize, height as usize];
        println!(
            "Constructing image from bytes: width: {}, height: {}",
            width, height,
        );
        println!("len of bytes: {}", bytes.len());
        let color_image = ColorImage::from_rgba_unmultiplied(size, &bytes);

        Self {
            image: Some(color_image),
            texture: None,
            sent: false,
        }
    }

    pub fn get_image(&self) -> Option<ColorImage> {
        self.image.clone()
    }

    pub fn default() -> Self {
        Self {
            image: None,
            texture: None,
            sent: false,
        }
    }
}

#[derive(Clone)]
pub struct ChatApp {
    app_type: AppType,
    chat_input: String,
    chat_history: ChatHistory,

    tx_input: AsyncSender,
    rx_input: AsyncReceiver,
    tx_stream: AsyncSender,
    rx_stream: AsyncReceiver,
    log: Log,

    server_addr: String,
    connection_status: ConnectionStatus,
    disconect_notify: Arc<Notify>,

    profile_pic_path: String,
    my_profile: ProfileType,
    target_profile: ProfileType,
}

impl ChatApp {
    pub fn new(app_type: AppType) -> Self {
        let (tx_input, rx_stream) = mpsc::channel(32);
        let (tx_stream, rx_input) = mpsc::channel(32);
        ChatApp {
            app_type,
            chat_input: String::new(),
            log: Arc::new(StdMutex::new(Vec::new())),
            chat_history: Arc::new(StdMutex::new(Vec::new())),
            tx_input: Arc::new(AsyncMutex::new(tx_input)),
            rx_input: Arc::new(AsyncMutex::new(rx_input)),
            tx_stream: Arc::new(AsyncMutex::new(tx_stream)),
            rx_stream: Arc::new(AsyncMutex::new(rx_stream)),
            server_addr: "".to_string(),
            connection_status: ConnectionStatus::DISCONNECTED,
            disconect_notify: Arc::new(Notify::new()),

            my_profile: Arc::new(StdMutex::new(Profile::default())),
            target_profile: Arc::new(StdMutex::new(Profile::default())),

            profile_pic_path: "".to_string(),
        }
    }

    pub fn set_profile_pic_path(&mut self, profile_pic_path: String) {
        self.profile_pic_path = profile_pic_path;
    }

    fn init_connection(&self) {
        let app_type = self.app_type.clone();
        let tx_stream_clone = self.tx_stream.clone();
        let rx_stream_clone = self.rx_stream.clone();
        let notify_clone = self.disconect_notify.clone();
        let server_addr_clone = self.server_addr.clone();
        tokio::spawn(async move {
            Self::init_connection_internal(
                app_type,
                tx_stream_clone,
                rx_stream_clone,
                notify_clone,
                server_addr_clone,
            )
            .await
        });

        let cache = self.chat_history.clone();
        let log = self.log.clone();
        let rx_input_clone = self.rx_input.clone();
        let mut target_profile = self.target_profile.clone();

        tokio::spawn(async move {
            Self::poll_messages(rx_input_clone, cache, log, &mut target_profile).await;
        });
    }

    async fn init_connection_internal(
        app_type: AppType,
        tx: AsyncSender,
        rx: AsyncReceiver,
        disconnect_notify: Arc<Notify>,
        server_addr: String,
    ) -> Result<(), std::io::Error> {
        if app_type == AppType::SERVER {
            Self::init_connection_server(tx.clone(), rx.clone(), disconnect_notify, server_addr)
                .await?;
        } else {
            Self::init_connection_client(tx.clone(), rx.clone(), disconnect_notify, server_addr)
                .await?;
        }

        Ok(())
    }

    async fn init_connection_client(
        tx_stream: AsyncSender,
        rx_stream: AsyncReceiver,
        disconnect_notify: Arc<Notify>,
        server_addr: String,
    ) -> Result<(), std::io::Error> {
        println!("Connecting to server");

        let server_addr_clone = server_addr.clone();

        let stream = match TcpStream::connect(server_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                println!("Failed to connect to server {}", e);
                return Err(e);
            }
        };
        println!("Connected to server {}", server_addr_clone);

        {
            let connect_message =
                construct_connection_message(connection_status::ConnectionStatus::CONNECTED);
            let tx_guard = tx_stream.lock().await;
            tx_guard.send(connect_message).await.unwrap();
        }

        handle_connection(
            stream,
            tx_stream.clone(),
            rx_stream.clone(),
            disconnect_notify,
        )
        .await;

        Ok(())
    }

    pub fn set_server_addr(&mut self, server_addr: String) {
        self.server_addr = server_addr;
    }

    async fn init_connection_server(
        tx_stream: AsyncSender,
        rx_stream: AsyncReceiver,
        disconnect_notify: Arc<Notify>,
        server_addr: String,
    ) -> Result<(), std::io::Error> {
        println!("Starting server on {}", server_addr);
        // let listener = match TcpListener::bind(server_addr).await {
        let listener = match TcpListener::bind("0.0.0.0:8888").await {
            Ok(listener) => listener,
            Err(e) => {
                println!("Failed to bind to port");
                return Err(e);
            }
        };

        println!("Listening for incoming connections");
        {
            let listening_message =
                construct_connection_message(connection_status::ConnectionStatus::LISTENING);
            let tx_guard = tx_stream.lock().await;
            tx_guard.send(listening_message).await.unwrap();
        }

        tokio::select! {
            Ok((stream, _addr)) = listener.accept() => {
                handle_connection(
                    stream,
                    tx_stream.clone(),
                    rx_stream.clone(),
                    disconnect_notify,
                ).await
            }
            _ = disconnect_notify.notified() => {
                let tx_guard = tx_stream.lock().await;
                let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
                tx_guard.send(disconnect_message).await.unwrap();
            }
        }

        Ok(())
    }

    async fn send_message(tx_input: AsyncSender, message: String, chat_history: ChatHistory) {
        let message_clone = message.clone();
        match tx_input
            .lock()
            .await
            .send(construct_text_message_generic(message, true))
            .await
        {
            Ok(_) => {
                let mut chat_history = chat_history.lock().unwrap();
                let new_message = construct_text_message(message_clone, true);
                chat_history.push(new_message);
            }
            Err(_) => println!("Failed to send message"),
        }
    }

    // Async function to poll for messages and update the synchronous cache
    async fn poll_messages(
        rx: AsyncReceiver,
        cache: ChatHistory,
        log: Log,
        target_profile: &mut ProfileType,
    ) {
        while let Some(generic_message) = rx.lock().await.recv().await {
            match generic_message {
                CombinedMessage::Message(message) => match message {
                    Message::TextMessage(text_message) => {
                        let mut cache_guard = cache.lock().unwrap();
                        cache_guard.push(Message::TextMessage(text_message.clone()));
                    }
                    Message::ImageMessage(image_message) => {
                        let content = image_message.content.clone();
                        let width = image_message.width;
                        let height = image_message.height;
                        let profile = Profile::from_bytes(content, width, height);

                        let mut target_profile_guard = target_profile.lock().unwrap();

                        *target_profile_guard = profile;
                    }
                },
                CombinedMessage::LogMessage(log_message) => {
                    let mut log_guard = log.lock().unwrap();
                    log_guard.push(log_message.clone());
                }
            };
        }
    }

    async fn send_my_profile(tx_input: AsyncSender, filepath: &str) {
        let mut size_reduction = 0.3;
        if filepath == "data/pojzo.jpg" {
            size_reduction = 0.2;
        }

        let profile = Profile::new(filepath, size_reduction);
        let image = profile.get_image().unwrap();
        let bytes = color_image_to_bytes(&image);
        let width = image.width() as u16;
        let height = image.height() as u16;

        let message = construct_image_message_generic(bytes, width, height);
        let tx_guard = tx_input.lock().await;

        tx_guard.send(message).await.unwrap();
    }
}

impl ChatApp {
    fn show_chat_history_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::none()
                .outer_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.heading("Chat History:");
                });
            egui::ScrollArea::vertical().show(ui, |ui| {
                let history_clone = self.chat_history.clone();
                let history_guard = history_clone.lock().unwrap();

                let log_guard = self.log.lock().unwrap();
                if !log_guard.is_empty() {
                    let log_guard_clone = log_guard.clone();
                    let last_log = log_guard_clone.last().unwrap();
                    match last_log {
                        LogMessage::ConnectionMessage(connection_message) => {
                            self.connection_status = connection_message.connection_status.clone();
                        }
                    }
                }

                let history_guard: Vec<&TextMessage> = history_guard
                    .iter()
                    .filter_map(|message| match message {
                        Message::TextMessage(text_message) => Some(text_message),
                        Message::ImageMessage(_) => None,
                    })
                    .collect();

                for message_struct in history_guard {
                    let message = message_struct.content.clone();
                    let time_sent = message_struct.time_sent.clone();
                    let full_message = format!("{}: {}", time_sent, message);
                    let color = if message_struct.sent {
                        (255, 0, 0)
                    } else {
                        (0, 255, 0)
                    };
                    ui.label(
                        egui::RichText::new(full_message)
                            .heading()
                            .color(egui::Color32::from_rgb(color.0, color.1, color.2)),
                    );
                }
            });
        });
    }

    fn show_status_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("Status panel").show(ctx, |ui| {
            ui.heading("Connection status");

            ui.horizontal(|ui| {
                ui.label("Server Address:");
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.server_addr);
            });

            let button_text = match self.connection_status {
                ConnectionStatus::CONNECTED => "Disconnect",
                ConnectionStatus::DISCONNECTED => {
                    if self.app_type == AppType::SERVER {
                        "Start server"
                    } else {
                        "Connect"
                    }
                }
                ConnectionStatus::CONNECTING => "",
                ConnectionStatus::FAILED => "Retry connect",
                ConnectionStatus::LISTENING => {
                    if self.app_type == AppType::SERVER {
                        "Stop listening"
                    } else {
                        ""
                    }
                }
            };

            let mut connection_text = String::new();
            match self.app_type {
                AppType::SERVER => match self.connection_status {
                    ConnectionStatus::CONNECTED => {
                        connection_text = "Connected".to_string();
                    }
                    ConnectionStatus::DISCONNECTED => {
                        connection_text = "Disconnected".to_string();
                    }
                    ConnectionStatus::LISTENING => {
                        connection_text = format!("Listening on {}", self.server_addr);
                    }
                    _ => {}
                },

                AppType::CLIENT => match self.connection_status {
                    ConnectionStatus::CONNECTED => {
                        connection_text = format!("Connected to {}", self.server_addr);
                    }
                    ConnectionStatus::DISCONNECTED => {
                        connection_text = "Disconnected".to_string();
                    }
                    ConnectionStatus::CONNECTING => {
                        connection_text = "Connecting...".to_string();
                    }
                    ConnectionStatus::FAILED => {
                        connection_text = "Failed to connect".to_string();
                    }
                    _ => {}
                },
            }
            ui.label(connection_text);
            if ui.button(button_text).clicked() {
                match self.connection_status {
                    ConnectionStatus::CONNECTED => {
                        self.disconect_notify.notify_waiters();
                    }
                    ConnectionStatus::DISCONNECTED => {
                        self.init_connection();
                    }
                    ConnectionStatus::LISTENING => {
                        self.disconect_notify.notify_waiters();
                    }
                    ConnectionStatus::CONNECTING => todo!(),
                    ConnectionStatus::FAILED => todo!(),
                }
            };
        });
    }

    fn show_chat_input_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("chat_input_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Message:");
                ui.text_edit_singleline(&mut self.chat_input);

                let message = self.chat_input.clone();
                if self.connection_status == ConnectionStatus::CONNECTED
                    && ui.button("Send").clicked()
                {
                    let tx_input = self.tx_input.clone();
                    let chat_history = self.chat_history.clone();
                    tokio::spawn(async {
                        Self::send_message(tx_input, message, chat_history).await;
                    });
                }
            });
        });
    }

    fn show_profile_image(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {

        // let my_profile_guard = self.my_profile.lock().unwrap();
        // let mut my_profile = my_profile_guard.clone();
        // if self.connection_status == ConnectionStatus::CONNECTED || true {
        //     if my_profile.image.is_none() {
        //         my_profile = Some(Profile::new("data/pojzo.jpg"));
        //     }

        //     if let Some(my_profile) = my_profile {
        //         if let Some(image) = &my_profile.image {
        //             let texture = Some(ctx.load_texture(
        //                 "my_image",
        //                 image.clone(),
        //                 egui::TextureOptions::default(),
        //             ));
        //             if let Some(texture) = &texture {
        //                 ui.image(texture);
        //             }
        //         }
        //     }
        // }
    }

    pub fn show_target_profile_image(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let profile_guard = self.target_profile.lock().unwrap();
        if let Some(image) = profile_guard.image.clone() {
            let texture = Some(ctx.load_texture(
                "target_image",
                image.clone(),
                egui::TextureOptions::default(),
            ));
            if let Some(texture) = &texture {
                ui.image(texture);
            }
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Show chat history panel on the left
                self.show_chat_history_panel(ctx);
                self.show_status_panel(ctx);
                if self.connection_status == ConnectionStatus::CONNECTED || true {
                    egui::SidePanel::right("profile_panel").show(ctx, |ui| {
                        self.show_target_profile_image(ctx, ui);
                    });
                }
                if self.connection_status == ConnectionStatus::CONNECTED {
                    let mut my_profile_guard = self.my_profile.lock().unwrap();
                    if my_profile_guard.sent == false {
                        let tx_input_clone = self.tx_input.clone();
                        if !self.profile_pic_path.is_empty() {
                            let profile_pic_path = self.profile_pic_path.clone();
                            my_profile_guard.sent = true;
                            drop(my_profile_guard);
                            let profile_pic_path_clone = profile_pic_path.clone();
                            tokio::spawn(async move {
                                Self::send_my_profile(tx_input_clone, &profile_pic_path_clone)
                                    .await;
                            });
                        }
                    }
                }
            });
        });

        // Show chat input panel at the bottom
        self.show_chat_input_panel(ctx);
    }
}
