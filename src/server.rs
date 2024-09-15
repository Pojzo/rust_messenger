use crate::app::app::ChatApp;
use enums::app_type::AppType;

mod app;

mod enums;
mod network;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Server application",
        options,
        Box::new(move |cc| {
            let mut app = ChatApp::new(AppType::SERVER);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            app.set_server_addr("0.0.0.0:8888".to_string());
            app.set_profile_pic_path("data/pojzo.jpg".to_string());
            Ok(Box::new(app))
        }),
    )
}
