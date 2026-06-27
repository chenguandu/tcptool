/// Right panel - Message details and quick commands
use eframe::egui::Ui;

pub fn show(ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.heading("📄 报文详情 / 快捷指令");
        ui.separator();
        ui.label("(选中报文后显示详情)");
    });
}
