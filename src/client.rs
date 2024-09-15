use enums::app_type::AppType;

use crate::app::app::ChatApp;

mod app;
mod enums;
mod network;
// mod stream_utils;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Client application",
        options,
        Box::new(move |_cc| {
            let mut app = ChatApp::new(AppType::CLIENT);
            app.set_server_addr("192.168.100.78:8888".to_string());
            // app.set_profile_pic_path("data/pojzo.jpg".to_string());
            Ok(Box::new(app))
        }),
    )
}
