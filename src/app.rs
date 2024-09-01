use eframe::egui;
use std::io;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;

use tokio::sync::{mpsc, Notify};

use std::sync::Mutex;
use std::time::Duration;
use tokio::time::{self, sleep};

use crate::common::AppType;
use crate::connection_status::ConnectionStatus;
use crate::message::Message;
use crate::stream_utils::stream_read;

type ChatHistory = Arc<Mutex<Vec<Message>>>;
type Sender = Arc<Mutex<mpsc::Sender<String>>>;
type Receiver = Arc<Mutex<mpsc::Receiver<String>>>;
type WriteStream = Arc<Mutex<Option<OwnedWriteHalf>>>;
type App = Arc<Mutex<ChatApp>>;
type SharedStateType = Arc<Mutex<SharedState>>;

#[derive(Clone)]
struct SharedState {
    chat_history: ChatHistory,
    tx: Sender,
    rx: Receiver,
    write_stream: WriteStream,
    server_addr: String,
    connected_to_addr: String,
    connection_status: ConnectionStatus,
}

#[derive(Clone)]
pub struct ChatApp {
    app_type: AppType,
    chat_input: String,
    log: Vec<String>,
    stop_signal: Arc<Notify>,
    shared_state: SharedStateType,
}

impl ChatApp {
    pub fn new(app_type: AppType) -> Self {
        let (tx, rx) = mpsc::channel(32);
        let shared_state = Arc::new(Mutex::new(SharedState {
            chat_history: Arc::new(Mutex::new(Vec::new())),
            tx: Arc::new(Mutex::new(tx)),
            rx: Arc::new(Mutex::new(rx)),
            write_stream: Arc::new(Mutex::new(None)),
            server_addr: "".to_string(),
            connected_to_addr: "".to_string(),
            connection_status: ConnectionStatus::DISCONNECTED,
        }));

        ChatApp {
            app_type,
            chat_input: String::new(),
            log: Vec::new(),
            stop_signal: Arc::new(Notify::new()),
            shared_state,
        }
    }
    // pub fn set_serer_addr(&mut self, addr: String) -> bool {
    //     if self.app_type == AppType::CLIENT {
    //         return false;
    //     }
    //     self.server_addr = addr;
    //     return true;
    // }

    /*
       async fn retry_connection(app: App, notify: Arc<Notify>) -> Result<(), io::Error> {
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
    */

    fn init_connection(&self) {
        let shared_state_clone = self.shared_state.clone();
        let app_type = self.app_type.clone();
        tokio::spawn(async move {
            println!("Initializing connection");
            Self::init_connection_internal(shared_state_clone, app_type).await;
        });
    }

    async fn init_connection_internal(
        shared_state: SharedStateType,
        app_type: AppType,
    ) -> Result<(), std::io::Error> {
        {
            let mut shared_state_guard = shared_state.lock().unwrap();
            shared_state_guard.connection_status = ConnectionStatus::CONNECTED;
        }

        if app_type == AppType::SERVER {
            Self::init_connection_server(shared_state).await?;
        } else {
            Self::init_connection_client(shared_state).await?;
        }

        Ok(())
    }

    async fn init_connection_client(shared_state: SharedStateType) -> Result<(), std::io::Error> {
        println!("Connecting to server");
        let stream = TcpStream::connect("127.0.0.1:8888").await?;

        // Accepting an incoming connection
        let (read_stream, write_stream) = stream.into_split();
        println!("Connected to server");
        let shared_state_guard = shared_state.lock().unwrap();
        *shared_state_guard.write_stream.lock().unwrap() = Some(write_stream);

        let app_guard = shared_state_guard.clone();
        tokio::spawn(async move {
            Self::poll_messages(app_guard.clone().rx, app_guard.clone().chat_history).await;
        });

        stream_read(read_stream, shared_state_guard.tx.clone()).await;

        Ok(())
    }

    async fn init_connection_server(shared_state: SharedStateType) -> Result<(), std::io::Error> {
        let server_addr;
        {
            let mut shared_state_guard = shared_state.lock().unwrap();
            server_addr = shared_state_guard.server_addr.clone();
            shared_state_guard.connection_status = ConnectionStatus::LISTENING;
            println!(
                "Set connection status to {}",
                shared_state_guard.connection_status
            );
        }
        let listener = TcpListener::bind(server_addr).await?;

        // let listener = TcpListener::bind("0.0.0.0:8888").await?;
        println!("Listening for incoming connections");
        let (stream, addr) = listener.accept().await?;

        {
            let mut app_guard = shared_state.lock().unwrap();
            app_guard.connected_to_addr = addr.to_string();
            app_guard.connection_status = ConnectionStatus::CONNECTED;
        }

        let (read_stream, write_stream) = stream.into_split();
        println!("Accepted connection from: {:?}", addr);
        {
            let app_guard = shared_state.lock().unwrap();
            println!("Connection type here: {}", app_guard.connection_status);
            *app_guard.write_stream.lock().unwrap() = Some(write_stream);
        }
        let shared_state_clone = shared_state.clone();
        tokio::spawn(async move {
            let shared_state_guard = shared_state_clone.lock().unwrap();
            let rx_clone = shared_state_guard.rx.clone();
            let chat_history_clone = shared_state_guard.chat_history.clone();
            drop(shared_state_guard); // Ensure the lock is released before polling messages
            Self::poll_messages(rx_clone, chat_history_clone).await;
        });

        {
            let shared_state_guard = shared_state.lock().unwrap();
            stream_read(read_stream, shared_state_guard.tx.clone()).await;
        }

        Ok(())
    }

    async fn poll_messages(rx: Receiver, chat_history: ChatHistory) {
        let mut interval = time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;

            let mut rx = rx.lock().unwrap();

            match rx.recv().await {
                Some(message) => {
                    println!("Received message: {}", message);
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
        let handle = Handle::current();
        handle.block_on(async {
            let shared_state_guard = self.shared_state.lock().await;
            shared_state_guard
                .clone()
                .chat_history
                .lock()
                .unwrap()
                .clone()
        })
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

                if ui.button("Send").clicked() {
                    if !self.chat_input.is_empty() {
                        let chat_input_clone = self.chat_input.clone();
                        let write_stream_copy = self.write_stream.clone();
                        let chat_history_clone = self.chat_history.clone();

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

                        self.chat_input.clear();
                    }
                }
            });
        });
    }
}
