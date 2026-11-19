//! Profile creation and deletion modal dialogs.

use crate::*;

impl FantasTechApp {
    pub(crate) fn render_create_profile_modal(&mut self, ctx: &egui::Context) {
        let domain_label = match self.profile_modal_domain {
            ProfileDomain::Button => "Button / Keymap",
            ProfileDomain::Lighting => "Lighting",
        };
        let mut do_create = false;
        let mut do_cancel = false;

        egui::Window::new(format!("Create New {} Profile", domain_label))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(RichText::new(format!("New {} Profile", domain_label)).strong());
                ui.separator();
                ui.add_space(4.0);

                ui.label(RichText::new("Profile Name:").strong());
                let edit_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.profile_modal_name)
                        .hint_text("e.g. Gaming, Work, FPS, Neon")
                        .desired_width(ui.available_width()),
                );
                if self.profile_modal_name.is_empty() {
                    edit_resp.request_focus();
                }
                if edit_resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    do_create = true;
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Base Configuration:").strong());
                ui.radio_value(&mut self.profile_modal_copy_current, true, "Copy current active settings");
                ui.radio_value(&mut self.profile_modal_copy_current, false, "Start with default factory settings");

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let can_submit = !self.profile_modal_name.trim().is_empty();
                    let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.add_enabled(can_submit, egui::Button::new(RichText::new("Create Profile").strong())).clicked()
                        || (can_submit && enter_pressed)
                    {
                        do_create = true;
                    }
                    if ui.button("Cancel (Esc)").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        do_cancel = true;
                    }
                });
            });

        if do_create && !self.profile_modal_name.trim().is_empty() {
            let name = self.profile_modal_name.clone();
            let copy = self.profile_modal_copy_current;
            match self.profile_modal_domain {
                ProfileDomain::Button => self.create_new_button_profile(&name, copy),
                ProfileDomain::Lighting => self.create_new_lighting_profile(&name, copy),
            }
            self.profile_modal_open = false;
            self.profile_modal_name.clear();
        } else if do_cancel {
            self.profile_modal_open = false;
            self.profile_modal_name.clear();
        }
    }

    pub(crate) fn render_delete_profile_modal(&mut self, ctx: &egui::Context) {
        let domain_label = match self.delete_profile_domain {
            ProfileDomain::Button => "button",
            ProfileDomain::Lighting => "lighting",
        };
        let mut do_delete = false;
        let mut do_cancel = false;

        egui::Window::new("Confirm Delete Profile")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(RichText::new("Delete Profile?").color(Color32::from_rgb(239, 68, 68)).strong());
                ui.separator();
                ui.add_space(4.0);

                ui.label(format!(
                    "Are you sure you want to permanently delete the custom {} profile '{}'?",
                    domain_label, self.delete_profile_name
                ));
                ui.label(RichText::new("This action cannot be undone.").size(11.0).italics().color(Color32::from_rgb(148, 163, 184)));

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Yes, Delete").color(Color32::from_rgb(239, 68, 68))).clicked() {
                        do_delete = true;
                    }
                    if ui.button("Cancel (Esc)").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        do_cancel = true;
                    }
                });
            });

        if do_delete {
            let id = self.delete_profile_id.clone();
            match self.delete_profile_domain {
                ProfileDomain::Button => self.delete_button_profile_by_id(&id),
                ProfileDomain::Lighting => self.delete_lighting_profile_by_id(&id),
            }
            self.delete_profile_modal_open = false;
            self.delete_profile_id.clear();
            self.delete_profile_name.clear();
        } else if do_cancel {
            self.delete_profile_modal_open = false;
            self.delete_profile_id.clear();
            self.delete_profile_name.clear();
        }
    }


}
