use super::*;

#[cfg(not(target_os = "linux"))]
impl ChatComposer {
    pub fn update_recording_meter_in_place(&mut self, id: &str, text: &str) -> bool {
        self.textarea.update_named_element_by_id(id, text)
    }

    pub fn insert_recording_meter_placeholder(&mut self, text: &str) -> String {
        let id = self.next_id();
        self.textarea.insert_named_element(text, id.clone());
        id
    }

    pub fn remove_recording_meter_placeholder(&mut self, id: &str) {
        let _ = self.textarea.replace_element_by_id(id, "");
    }
}
