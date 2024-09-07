use enums::app_type::AppType;

use crate::app::app::ChatApp;

mod app;

mod enums;
mod network;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Server application",
        options,
        Box::new(move |_cc| {
            let mut app = ChatApp::new(AppType::SERVER);
            app.set_server_addr("0.0.0.0:8888".to_string());
            Box::new(app)
        }),
    )
}
