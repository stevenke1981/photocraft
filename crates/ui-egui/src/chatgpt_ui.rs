//! A focused native image dialog. Prompt and window state use the existing serializable dialog
//! fields; connection checks and generation dispatch engine commands and background jobs.
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;

pub fn owns(fields: &Map<String, Value>) -> bool {
    fields.get("__chatgpt").and_then(Value::as_bool) == Some(true)
}

pub fn open(app: &mut PhotocraftApp) -> u64 {
    let mut fields = Map::new();
    fields.insert("__chatgpt".into(), json!(true));
    fields.insert("__command".into(), json!("imagegen.generate"));
    fields.insert("__label".into(), json!("ChatGPT Images…"));
    fields.insert("prompt".into(), json!(""));
    app.ui.open_dialog(DialogKind::Command, fields)
}

pub fn can_generate(app: &PhotocraftApp, fields: &Map<String, Value>) -> bool {
    app.session.is_enabled("imagegen.generate")
        && !app.session.has_jobs()
        && fields.get("prompt").and_then(Value::as_str).is_some_and(photocraft_engine::imagegen_cmds::valid_prompt)
}

pub fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, fields: &mut Map<String, Value>) {
    ui.weak(tl!("Experimental · ChatGPT subscription"));
    ui.add_space(12.0);
    ui.label(tl!("Describe your image"));
    let mut prompt = fields.get("prompt").and_then(Value::as_str).unwrap_or("").to_string();
    ui.add(
        egui::TextEdit::multiline(&mut prompt)
            .desired_width(f32::INFINITY)
            .desired_rows(5)
            .char_limit(8000)
            .hint_text(tl!("Describe the subject, setting and style")),
    );
    fields.insert("prompt".into(), json!(prompt));
    ui.add_space(12.0);
    ui.label(&app.session.imagegen.status.message);
    if let Some(plan) = &app.session.imagegen.status.plan {
        ui.weak(crate::i18n::fmt(tl!("Plan: {plan}"), &[("plan", plan)]));
    }
    ui.add_space(8.0);
    ui.add_enabled_ui(!app.session.has_jobs(), |ui| {
        if crate::widgets::secondary_button(ui, tl!("Check connection"), 140.0).clicked()
            && let Err(error) = app.run("imagegen.status", json!({}))
        {
            app.session.imagegen.status.ready = false;
            app.session.imagegen.status.message = error;
        }
    });
    ui.add_space(12.0);
    ui.weak(tl!("Uses included Codex allowance. No API key or paid fallback."));
    ui.weak(tl!("Images use more of your included allowance."));
}

pub fn confirm(app: &mut PhotocraftApp, fields: &Map<String, Value>) -> Result<Value, String> {
    if !can_generate(app, fields) {
        return Err("Check the ChatGPT connection and enter an image description before generating.".into());
    }
    app.run("imagegen.generate", json!({"prompt":fields.get("prompt")}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_engine::Session;

    #[test]
    fn chatgpt_ui_disabled_and_exhausted_connections_cannot_confirm() {
        let mut app = PhotocraftApp::new(Session::default(), crate::Services::default());
        let id = open(&mut app);
        let mut fields = app.ui.dialog_mut(id).unwrap().fields.clone();
        fields.insert("prompt".into(), json!("A ceramic mug"));
        assert!(!can_generate(&app, &fields));
        assert!(confirm(&mut app, &fields).is_err());
        app.session.imagegen.status.message = "Included subscription allowance is exhausted".into();
        assert!(!can_generate(&app, &fields));
        assert!(app.session.documents().is_empty());
        assert_eq!(serde_json::to_value(&app.ui).unwrap()["dialogs"][0]["fields"]["__chatgpt"], true);
    }
}
