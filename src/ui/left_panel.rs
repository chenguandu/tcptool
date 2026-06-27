/// Left panel - Connection list
use eframe::egui::Ui;

pub fn show(ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.heading("📡 连接列表");
        ui.separator();
        ui.label("(暂无连接)");
    });
}
