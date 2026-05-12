use saveui::SaveUiApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1120.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Soft Souls Saver",
        options,
        Box::new(|creation_context| Ok(Box::new(SaveUiApp::new(creation_context)))),
    )
}
