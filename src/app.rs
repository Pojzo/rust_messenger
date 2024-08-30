use eframe::egui;
use std::io;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;

use tokio::sync::{mpsc, Mutex as AsyncMutex, Notify};

use std::sync::Mutex;
use std::time::Duration;
use tokio::time::{self, sleep};

use crate::common::AppType;
use crate::connection_status::ConnectionStatus;
use crate::stream_utils::stream_read;

type ChatHistory = Arc<Mutex<Vec<Message>>>;
type Sender = Arc<AsyncMutex<mpsc::Sender<String>>>;
type Receiver = Arc<AsyncMutex<mpsc::Receiver<String>>>;
type WriteStream = Arc<AsyncMutex<Option<OwnedWriteHalf>>>;

#[derive(Clone)]
pub struct ChatApp {
    app_type: AppType,
    chat_input: String,
    chat_history: ChatHistory,
    connection_status: ConnectionStatus,
    tx: Sender,
    rx: Receiver,
    write_stream: WriteStream,
    stop_signal: Arc<Notify>,
}

impl ChatApp {
    pub fn new(app_type: AppType) -> Self {
        let (tx, rx) = mpsc::channel(32);

        ChatApp {
            chat_input: String::new(),
            chat_history: Arc::new(Mutex::new(Vec::new())),
            app_type,
            connection_status: ConnectionStatus::DISCONNECTED,
            tx: Arc::new(AsyncMutex::new(tx)),
            rx: Arc::new(AsyncMutex::new(rx)),
            write_stream: Arc::new(AsyncMutex::new(None)),
            stop_signal: Arc::new(Notify::new()),
        }
    }

    fn init(&self) {
        let rx = self.rx.clone();
        let chat_history = self.chat_history.clone();

        // tokio::spawn(async move {
        //     Self::poll_messages(rx, chat_history).await;
        // });

        println!("This init was called");
    }

    pub fn init_connection(&mut self) {
        self.connection_status = ConnectionStatus::CONNECTING;
        let app = Arc::new(AsyncMutex::new(self.clone()));
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();

        let mut failed = false;

        tokio::spawn(async move {
            let result = Self::retry_connection(app, notify_clone).await;
            if let Err(e) = result {
                eprintln!("Failed to establish connection: {:?}", e);
                failed = true;
            }
        });

        let _ = notify.notified();
        println!("got after the notify");
        if failed {
            self.connection_status = ConnectionStatus::FAILED;
        }
    }

    async fn retry_connection(
        app: Arc<AsyncMutex<ChatApp>>,
        notify: Arc<Notify>,
    ) -> Result<(), io::Error> {
        let retries_threshold = 5;
        let retry_delay = Duration::from_secs(2);

        for attempt in 1..=retries_threshold {
            let app_clone = app.clone();

            // Try to initialize connection
            let result = Self::init_connection_internal(app_clone).await;
            if result.is_ok() {
                println!("Successfully established connection on attempt {}", attempt);
                notify.notify_one(); // Notify that connection was successful
                return Ok(());
            } else {
                eprintln!(
                    "Failed to initialize connection on attempt {}: {:?}",
                    attempt,
                    result.err().unwrap()
                );
                if attempt < retries_threshold {
                    sleep(retry_delay).await; // Wait before the next attempt
                }
            }
        }
        notify.notify_one();

        Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to establish connection after maximum retries",
        ))
    }

    async fn init_connection_internal(app: Arc<AsyncMutex<ChatApp>>) -> Result<(), std::io::Error> {
        // Binding the TcpListener to the address
        let stream = TcpStream::connect("0.0.0.0:8888").await?;

        println!("Listening");

        // Accepting an incoming connection
        let (read_stream, write_stream) = stream.into_split();
        let app = app.lock().await;
        *app.write_stream.lock().await = Some(write_stream);

        stream_read(read_stream, app.tx.clone()).await;

        Ok(())
    }

    async fn poll_messages(rx: Receiver, chat_history: ChatHistory) {
        let mut interval = time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;

            // Lock the async mutex for receiving messages
            let mut rx = rx.lock().await;

            match rx.recv().await {
                Some(message) => {
                    // Lock the sync mutex for chat history in a blocking way
                    let mut history = chat_history.lock().unwrap();
                    let message = Message::new(message, false);
                    history.push(message);
                }
                None => break,
            }
        }
    }

    fn get_chat_history(&self) -> Vec<Message> {
        // Attempt to acquire the lock and handle potential errors
        match self.chat_history.lock() {
            Ok(history) => history.clone(),
            Err(_) => {
                // Handle the error gracefully, e.g., return an empty history
                eprintln!("Failed to acquire lock on chat history");
                Vec::new()
            }
        }
    }

    async fn send_message(
        write_stream: &mut OwnedWriteHalf,
        chat_history: &ChatHistory,
        message: &str,
    ) {
        // Directly use the write_stream without moving it
        chat_history
            .lock()
            .unwrap()
            .push(Message::new(message.to_string(), true));

        if let Err(e) = write_stream.write_all(message.to_string().as_bytes()).await {
            eprintln!("Couldn't write data: {e}");
        }
    }
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("chat_history_panel").show(ctx, |ui| {
            ui.heading("Chat History:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                let history = Self::get_chat_history(&self);
                for message_struct in history {
                    let message = message_struct.content;
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
            };
            if ui.button(button_text).clicked() {
                self.init_connection();
            };
        });

        // Bottom panel for the chat input
        egui::TopBottomPanel::bottom("chat_input_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Message:");
                ui.text_edit_singleline(&mut self.chat_input);

                if ui.button("Send").clicked() {
                    if !self.chat_input.is_empty() {
                        let chat_input_clone = self.chat_input.clone();
                        let write_stream_copy = self.write_stream.clone();
                        let chat_history_clone = self.chat_history.clone();

                        /*
                        tokio::spawn(async move {
                            let mut write_stream_guard = write_stream_copy.lock().await;
                            if let Some(write_stream) = &mut *write_stream_guard {
                                Self::send_message(
                                    write_stream,
                                    &chat_history_clone,
                                    &chat_input_clone,
                                )
                                .await;
                            } else {
                                eprintln!("Write stream is not available");
                            }
                        });
                        */

                        self.chat_input.clear();
                    }
                }
            });
        });
    }
}

#[derive(Clone)]
struct Message {
    content: String,
    sent: bool,
}

impl Message {
    fn new(content: String, sent: bool) -> Self {
        Message { content, sent }
    }
}
