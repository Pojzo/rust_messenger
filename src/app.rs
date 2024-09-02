use eframe::egui;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;

use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};

use std::sync::Mutex as StdMutex;
use tokio::time::{self, sleep};

use crate::common::AppType;
use crate::connection_status::ConnectionStatus;
use crate::message::{CombinedMessage, LogMessage, Message, TextMessage};
use crate::stream_utils::{stream_read, stream_write};

type ChatHistory = Arc<StdMutex<Vec<Message>>>;
type Log = Arc<StdMutex<Vec<LogMessage>>>;

type AsyncSender = Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>;
type AsyncReceiver = Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>;

type App = Arc<StdMutex<ChatApp>>;

#[derive(Clone)]
pub struct ChatApp {
    app_type: AppType,
    chat_input: String,
    stop_signal: Arc<Notify>,
    chat_history: ChatHistory,

    tx_input: AsyncSender,
    rx_input: AsyncReceiver,
    tx_stream: AsyncSender,
    rx_stream: AsyncReceiver,
    log: Log,

    server_addr: String,
    connected_to_addr: String,
    connection_status: ConnectionStatus,
}

impl ChatApp {
    pub fn new(app_type: AppType) -> Self {
        let (tx_input, rx_stream) = mpsc::channel(32);
        let (tx_stream, rx_input) = mpsc::channel(32);
        ChatApp {
            app_type,
            chat_input: String::new(),
            log: Arc::new(StdMutex::new(Vec::new())),
            stop_signal: Arc::new(Notify::new()),
            chat_history: Arc::new(StdMutex::new(Vec::new())),
            tx_input: Arc::new(AsyncMutex::new(tx_input)),
            rx_input: Arc::new(AsyncMutex::new(rx_input)),
            tx_stream: Arc::new(AsyncMutex::new(tx_stream)),
            rx_stream: Arc::new(AsyncMutex::new(rx_stream)),
            server_addr: "".to_string(),
            connected_to_addr: "".to_string(),
            connection_status: ConnectionStatus::DISCONNECTED,
        }
    }

    fn init_connection(&self) {
        let app_type = self.app_type.clone();
        let tx_stream_clone = self.tx_stream.clone();
        let rx_stream_clone = self.rx_stream.clone();

        tokio::spawn(async move {
            println!("Initializing connection");
            Self::init_connection_internal(app_type, tx_stream_clone, rx_stream_clone).await;
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
    ) -> Result<(), std::io::Error> {
        if app_type == AppType::SERVER {
            Self::init_connection_server(tx.clone(), rx.clone()).await?;
        } else {
            Self::init_connection_client(tx.clone(), rx.clone()).await?;
        }

        Ok(())
    }

    async fn init_connection_client(
        tx_stream: AsyncSender,
        rx_stream: AsyncReceiver,
    ) -> Result<(), std::io::Error> {
        println!("Connecting to server");
        let stream = TcpStream::connect("127.0.0.1:8888").await?;

        // Accepting an incoming connection
        let (read_stream, write_stream) = stream.into_split();

        tokio::join!(
            stream_read(read_stream, tx_stream.clone()),
            stream_write(write_stream, rx_stream.clone())
        );

        Ok(())
    }

    async fn init_connection_server(
        tx_stream: AsyncSender,
        rx_stream: AsyncReceiver,
    ) -> Result<(), std::io::Error> {
        let listener = TcpListener::bind("0.0.0.0:8888").await?;

        // let listener = TcpListener::bind("0.0.0.0:8888").await?;
        println!("Listening for incoming connections");
        let (stream, addr) = listener.accept().await?;

        let (read_stream, write_stream) = stream.into_split();

        println!("Accepted connection from: {:?}", addr);

        tokio::join!(
            stream_read(read_stream, tx_stream.clone()),
            stream_write(write_stream, rx_stream.clone())
        );

        Ok(())
    }

    async fn send_message(tx_input: AsyncSender, message: String, chat_history: ChatHistory) {
        let message_clone = message.clone();
        println!("Trying to write message: {}", message);
        match tx_input
            .lock()
            .await
            .send(CombinedMessage::Message(Message::TextMessage(
                TextMessage {
                    content: message,
                    sent: true,
                },
            )))
            .await
        {
            Ok(_) => {
                println!("Message sent");
                let mut cache = chat_history.lock().unwrap();
                cache.push(Message::TextMessage({
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

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("chat_history_panel").show(ctx, |ui| {
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

                // filter only TextMessage
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
        egui::SidePanel::right("Status panel").show(ctx, |ui| {
            ui.heading("Connection status");
            ui.label(&self.connection_status.to_string());

            let button_text = match self.connection_status {
                ConnectionStatus::CONNECTED => "Disconnect",
                ConnectionStatus::DISCONNECTED => "Connect",
                ConnectionStatus::CONNECTING => "",
                ConnectionStatus::FAILED => "Retry connect",
                ConnectionStatus::LISTENING => "Listening on {}",
            };

            let connection_text: String;
            match self.app_type {
                AppType::SERVER => {
                    connection_text = self.connected_to_addr.clone();
                }
                AppType::CLIENT => {
                    connection_text = format!("Connected to {}", self.connected_to_addr);
                }
            }
            ui.label(connection_text);
            if ui.button(button_text).clicked() {
                self.init_connection();
            };
        });

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
                } else {
                }
            });
        });
    }
}
