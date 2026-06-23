use super::BottomPane;

#[cfg(not(target_os = "linux"))]
impl BottomPane {
    pub(crate) fn insert_recording_meter_placeholder(&mut self, text: &str) -> String {
        let id = self.composer.insert_recording_meter_placeholder(text);
        self.composer.sync_popups();
        self.request_redraw();
        id
    }

    pub(crate) fn update_recording_meter_in_place(&mut self, id: &str, text: &str) -> bool {
        let updated = self.composer.update_recording_meter_in_place(id, text);
        if updated {
            self.composer.sync_popups();
            self.request_redraw();
        }
        updated
    }

    pub(crate) fn remove_recording_meter_placeholder(&mut self, id: &str) {
        self.composer.remove_recording_meter_placeholder(id);
        self.composer.sync_popups();
        self.request_redraw();
    }
}
