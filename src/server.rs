use app::ChatApp;
use common::AppType;

mod app;

mod common;
mod connection_status;
mod message;
mod stream_utils;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Server application",
        options,
        Box::new(move |_cc| {
            let app = ChatApp::new(AppType::SERVER);
            Box::new(app)
        }),
    )
}
