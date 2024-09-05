use eframe::egui;
use std::fmt::format;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;

use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};

use std::sync::Mutex as StdMutex;

use crate::common::AppType;
use crate::connection_status::{self, ConnectionStatus};
use crate::message::{
    construct_connection_message, construct_text_message, CombinedMessage, LogMessage, Message,
    TextMessage,
};
use crate::stream_utils::{handle_connection, stream_read, stream_write};

type ChatHistory = Arc<StdMutex<Vec<Message>>>;
type Log = Arc<StdMutex<Vec<LogMessage>>>;

type AsyncSender = Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>;
type AsyncReceiver = Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>;

type App = Arc<StdMutex<ChatApp>>;

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
    connected_to_addr: String,
    connection_status: ConnectionStatus,
    disconect_notify: Arc<Notify>,
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
            connected_to_addr: "".to_string(),
            connection_status: ConnectionStatus::DISCONNECTED,
            disconect_notify: Arc::new(Notify::new()),
        }
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
        tokio::spawn(async {
            Self::poll_messages(rx_input_clone, cache, log).await;
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

        let stream = TcpStream::connect(server_addr).await?;

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
        println!("Starting server");
        let listener = match TcpListener::bind(server_addr).await {
            Ok(listener) => listener,
            Err(e) => {
                println!("Failed to bind to port");
                return Err(e);
            }
        };

        // let listener = TcpListener::bind("0.0.0.0:8888").await?;
        println!("Listening for incoming connections");
        {
            let listening_message =
                construct_connection_message(connection_status::ConnectionStatus::LISTENING);
            println!("Waiting for tx_stream lock");
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
                println!("I too have been received that");
                let tx_guard = tx_stream.lock().await;
                let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
                tx_guard.send(disconnect_message).await.unwrap();
            }
        }

        Ok(())
    }

    async fn send_message(tx_input: AsyncSender, message: String, chat_history: ChatHistory) {
        let message_clone = message.clone();
        println!("Trying to write message: {}", message);
        match tx_input
            .lock()
            .await
            .send(construct_text_message(message, true))
            .await
        {
            Ok(_) => {
                println!("Message sent");
                let mut chat_history = chat_history.lock().unwrap();
                chat_history.push(Message::TextMessage({
                    TextMessage {
                        content: message_clone,
                        sent: true,
                    }
                }));
            }
            Err(_) => println!("Failed to send message"),
        }
    }
    // Async function to poll for messages and update the synchronous cache
    async fn poll_messages(rx: AsyncReceiver, cache: ChatHistory, log: Log) {
        while let Some(generic_message) = rx.lock().await.recv().await {
            match generic_message {
                CombinedMessage::Message(message) => match message {
                    Message::TextMessage(text_message) => {
                        let mut cache_guard = cache.lock().unwrap();
                        cache_guard.push(Message::TextMessage(text_message.clone()));
                    }
                },
                CombinedMessage::LogMessage(log_message) => {
                    let mut log_guard = log.lock().unwrap();
                    log_guard.push(log_message.clone());
                }
            };
        }
    }
}

impl ChatApp {
    fn show_chat_history_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Chat History:");
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
                        _ => None,
                    })
                    .collect();

                for message_struct in history_guard {
                    let message = message_struct.content.clone();
                    let color = if message_struct.sent {
                        (255, 0, 0)
                    } else {
                        (0, 255, 0)
                    };
                    ui.label(
                        egui::RichText::new(message)
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
                if ui.button("Send").clicked() {
                    let tx_input = self.tx_input.clone();
                    let chat_history = self.chat_history.clone();
                    tokio::spawn(async {
                        Self::send_message(tx_input, message, chat_history).await;
                    });
                }
            });
        });
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.show_chat_history_panel(ctx);
                self.show_status_panel(ctx);
            });
        });
        self.show_chat_input_panel(ctx);
    }
}
