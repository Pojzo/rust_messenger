use eframe::egui;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::TcpListener;

use tokio::sync::{mpsc, Mutex as AsyncMutex};

use std::sync::Mutex;
use std::time::Duration;
use tokio::time;

mod common;
mod connection_status;
// mod server_mode

use connection_status::ConnectionStatus;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    // Create a runtime to run async tasks in the background
    // let runtime = Arc::new(Mutex::new(Runtime::new().unwrap()));

    eframe::run_native(
        "Simple Chat UI",
        options,
        Box::new(move |_cc| {
            let app = ChatApp::new();
            Box::new(app)
        }),
    )
}

struct ChatApp {
    chat_input: String,
    chat_history: Arc<Mutex<Vec<String>>>,
    connection_status: ConnectionStatus,
    tx: Arc<AsyncMutex<mpsc::Sender<String>>>,
    rx: Arc<AsyncMutex<mpsc::Receiver<String>>>,
}

impl ChatApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel(32);

        ChatApp {
            chat_input: String::new(),
            chat_history: Arc::new(Mutex::new(Vec::new())),
            connection_status: ConnectionStatus::DISCONNECTED,
            tx: Arc::new(AsyncMutex::new(tx)),
            rx: Arc::new(AsyncMutex::new(rx)),
        }
    }
    async fn send_message(tx: Arc<AsyncMutex<mpsc::Sender<String>>>, message: String) {
        println!("Sending message: {}", message);
        let tx_guard = tx.lock().await;
        tx_guard.send(message).await;
    }

    async fn poll_messages(
        rx: Arc<AsyncMutex<mpsc::Receiver<String>>>,
        chat_history: Arc<Mutex<Vec<String>>>,
    ) {
        let mut interval = time::interval(Duration::from_millis(200));
        loop {
            interval.tick().await;

            // Lock the async mutex for receiving messages
            let mut rx = rx.lock().await;

            match rx.recv().await {
                Some(message) => {
                    // Lock the sync mutex for chat history in a blocking way
                    let mut history = chat_history.lock().unwrap();
                    history.push(message);
                }
                None => break,
            }
            // let mut history = chat_history.lock().unwrap();
            // history.push("Bla bla".to_string());
        }
    }

    async fn handle_client_read(
        mut read_stream: OwnedReadHalf,
        tx: Arc<AsyncMutex<mpsc::Sender<String>>>,
    ) {
        let mut buffer = [0; 512];

        match read_stream.peer_addr() {
            Ok(addr) => {
                println!("Connected to: {}", addr);
            }
            Err(e) => {
                eprintln!("Failed to get peer address {}", e);
            }
        }
        loop {
            match read_stream.read(&mut buffer).await {
                Ok(0) => {
                    println!("Connection closed by the client.");
                    break;
                }
                Ok(n) => {
                    let tx_guard = tx.lock().await;
                    let message = String::from_utf8_lossy(&buffer[..n]);
                    match tx_guard.send(message.to_string()).await {
                        Ok(_) => {
                            println!("Received message: {}", message);
                        }
                        Err(e) => {
                            println!("Couldnt receive message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read from connection: {}", e);
                    break;
                }
            }
        }
    }

    fn init(&mut self) {
        let rx = self.rx.clone();
        let chat_history = self.chat_history.clone();

        tokio::spawn(Self::send_message(
            self.tx.clone(),
            String::from("First message"),
        ));

        tokio::spawn(Self::poll_messages(rx, chat_history));

        tokio::spawn(Self::init_server(self.tx.clone()));
        self.connection_status = ConnectionStatus::CONNECTING;
    }

    async fn init_server(tx: Arc<AsyncMutex<mpsc::Sender<String>>>) -> Result<(), std::io::Error> {
        // Binding the TcpListener to the address
        let listener = TcpListener::bind("0.0.0.0:8888").await?;
        println!("Listening");

        // Accepting an incoming connection
        if let Ok((stream, _)) = listener.accept().await {
            println!("Connected to client");
            let (read_stream, write_stream) = stream.into_split();
            Self::handle_client_read(read_stream, tx).await;
        }

        Ok(())
    }

    fn get_chat_history(&self) -> Vec<String> {
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
}

impl eframe::App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.connection_status == ConnectionStatus::DISCONNECTED {
            self.init();
        }
        egui::TopBottomPanel::top("chat_history_panel").show(ctx, |ui| {
            ui.heading("Chat History:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                let history = Self::get_chat_history(&self);
                for message in history {
                    ui.label(message);
                }
            });
        });
        egui::SidePanel::right("Status panel").show(ctx, |ui| {
            ui.heading("Connection status");
            ui.label(&self.connection_status.to_string());
        });

        // Bottom panel for the chat input
        egui::TopBottomPanel::bottom("chat_input_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Message:");
                ui.text_edit_singleline(&mut self.chat_input);

                if ui.button("Send").clicked() {
                    if !self.chat_input.is_empty() {
                        let tx_copy = self.tx.clone();
                        let chat_input_clone = self.chat_input.clone();

                        // tokio::spawn(async move { tx_copy.send(chat_input_clone).await });
                        self.chat_input.clear();
                    }
                }
            });
        });
    }
}
