use eframe::NativeOptions;

fn main() {
  eframe::run_native(
    "orin",
    NativeOptions::default(),
    Box::new(|_| Ok(Box::new(App {}))),
  )
  .unwrap();
}

struct App {}

impl eframe::App for App {
  fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
    dock
  }
}
