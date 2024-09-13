use egui::{ColorImage, TextureHandle};
use image::{DynamicImage, GenericImageView};
use std::io::{Bytes, Cursor};

fn load_image(filepath: &str) -> Result<DynamicImage, std::io::Error> {
    if let Ok(image) = image::open(filepath) {
        return Ok(image);
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to load image",
        ));
    }
}

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let filepath = "data/pojzo.jpg";

    let load_image_result = load_image(filepath);

    let image = match load_image_result {
        Ok(image) => image,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Ok(());
        }
    };

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Client application",
        options,
        Box::new(move |_cc| {
            let mut app = App::new(image);
            Ok(Box::new(app))
        }),
    );
    Ok(())
}

struct App {
    image: ColorImage,
    texture: Option<TextureHandle>,
}

impl App {
    pub fn new(image: DynamicImage) -> Self {
        let new_width = 300;
        let new_height = 300;

        let image = image.resize(new_width, new_height, image::imageops::FilterType::Nearest);

        let dim = image.dimensions();
        let size = [dim.0 as usize, dim.1 as usize];

        let color_image = ColorImage::from_rgba_unmultiplied(size, &image.to_rgba8().to_vec());

        Self {
            image: color_image,
            texture: None,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(texture) = &self.texture {
                ui.image(texture);
            } else {
                let texture = Some(ctx.load_texture(
                    "my_image",
                    self.image.clone(),
                    egui::TextureOptions::default(),
                ));
                self.texture = texture;
            }
        });
    }
}
