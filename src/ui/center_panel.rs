/// Center panel - Message stream display
use eframe::egui::Ui;

pub fn show(ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.heading("📋 报文流");
        ui.separator();
        ui.label("(暂无报文)");
    });
}
