//! Local subscription image generation, accessible through the same cancellable commands as
//! the UI and trusted local CLI. Generated images open as new unsaved documents.
use std::sync::Arc;

use photocraft_imagegen::{Provider, Status, validate_png, validate_prompt};
use serde_json::{Value, json};

use crate::commands::{CommandSpec, always};
use crate::{EngineError, Result, Session, jobs};

#[derive(Default)]
pub struct State {
    pub provider: Option<Arc<dyn Provider>>,
    pub status: Status,
}

pub fn valid_prompt(prompt: &str) -> bool {
    validate_prompt(prompt).is_ok()
}

impl State {
    pub fn local() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(provider) = photocraft_imagegen::CodexProvider::discover() {
            return Self {
                provider: Some(Arc::new(provider)),
                status: Status { message: "Check your Codex connection before generating an image.".into(), ..Status::default() },
            };
        }
        Self {
            provider: None,
            status: Status {
                message: "ChatGPT images require the native desktop app and official Codex CLI. Generation is disabled.".into(),
                ..Status::default()
            },
        }
    }
}

fn provider(s: &Session) -> Result<Arc<dyn Provider>> {
    s.imagegen.provider.clone().ok_or_else(|| EngineError::Other(s.imagegen.status.message.clone()))
}

fn available(s: &Session) -> std::result::Result<(), String> {
    if s.imagegen.provider.is_some() && s.imagegen.status.ready { Ok(()) } else { Err(s.imagegen.status.message.clone()) }
}

fn check(s: &mut Session, _: &Value) -> Result<Value> {
    let Some(provider) = s.imagegen.provider.clone() else {
        return Ok(json!(s.imagegen.status));
    };
    jobs::run(
        s,
        "Check ChatGPT connection",
        false,
        move |ctx| provider.status(&|| ctx.cancelled()).map_err(EngineError::Other),
        |s, status| {
            s.imagegen.status = status;
            Ok(json!(s.imagegen.status))
        },
    )
}

fn generate(s: &mut Session, params: &Value) -> Result<Value> {
    let prompt =
        params.get("prompt").and_then(Value::as_str).ok_or_else(|| EngineError::Other("imagegen.generate requires a prompt string".into()))?.to_string();
    validate_prompt(&prompt).map_err(EngineError::Other)?;
    let provider = provider(s)?;
    jobs::run(
        s,
        "Generate with ChatGPT",
        false,
        move |ctx| {
            ctx.progress(0.05, "Generating with your ChatGPT subscription");
            ctx.check()?;
            let bytes = provider.generate(&prompt, &|| ctx.cancelled()).map_err(EngineError::Other)?;
            ctx.check()?;
            validate_png(&bytes).map_err(EngineError::Other)?;
            let mut imported = photocraft_io::import("ChatGPT image.png", &bytes).map_err(|e| EngineError::Other(e.to_string()))?;
            imported.document.name = "ChatGPT image".into();
            ctx.progress(1.0, "Image ready");
            Ok(imported.document)
        },
        |s, doc| {
            let size = doc.size;
            let index = s.add_document(doc, None);
            if let Some(st) = s.active_mut() {
                st.saved_revision = 0;
            }
            Ok(json!({"index":index,"width":size.width,"height":size.height,"provider":"chatgptCodex","subscriptionOnly":true}))
        },
    )
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "imagegen.status",
            label: "Check ChatGPT connection",
            menu: &[],
            shortcut: None,
            params: "{}",
            enabled: always,
            run: check,
            journal: false,
        },
        CommandSpec {
            id: "imagegen.generate",
            label: "Generate with ChatGPT",
            menu: &[],
            shortcut: None,
            params: "{prompt:string} (local, experimental, included subscription only)",
            enabled: available,
            run: generate,
            journal: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::SampleType;

    struct Mock {
        fail: bool,
    }
    impl Provider for Mock {
        fn status(&self, _: &dyn Fn() -> bool) -> photocraft_imagegen::Result<Status> {
            Ok(Status { ready: true, plan: Some("mock".into()), message: "Test provider".into() })
        }
        fn generate(&self, _: &str, cancelled: &dyn Fn() -> bool) -> photocraft_imagegen::Result<Vec<u8>> {
            if self.fail || cancelled() {
                return Err("Mock generation failed".into());
            }
            // Original synthetic 2x1 RGBA8 PNG: red and blue pixels, no external asset/service.
            Ok(vec![
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 2, 0, 0, 0, 1, 8, 6, 0, 0, 0, 244, 34, 127, 138, 0, 0, 0, 14, 73, 68,
                65, 84, 120, 156, 99, 248, 207, 192, 0, 66, 255, 1, 15, 249, 3, 253, 133, 17, 153, 118, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
            ])
        }
    }

    fn mock(fail: bool) -> Session {
        let mut s = Session::default();
        s.imagegen.provider = Some(Arc::new(Mock { fail }));
        s.execute("imagegen.status", json!({})).unwrap();
        s
    }

    #[test]
    fn imagegen_mock_opens_an_editable_unsaved_document_without_touching_existing_work() {
        let mut s = mock(false);
        s.execute("file.new", json!({"width":4,"height":3,"depth":16})).unwrap();
        let old = s.active().unwrap().doc.clone();
        let result = s.execute("imagegen.generate", json!({"prompt":"A ceramic mug"})).unwrap();
        assert_eq!(result["width"], 2);
        assert_eq!(s.documents().len(), 2);
        assert!(Arc::ptr_eq(&old, &s.documents()[0].doc));
        assert_eq!(s.documents()[0].doc.depth, SampleType::U16);
        assert!(s.active().unwrap().is_dirty());
        assert_eq!(s.active().unwrap().path, None);
        s.execute("layer.new.layer", json!({"name":"Edit"})).unwrap();
        s.execute("edit.undo", json!({})).unwrap();
        s.execute("edit.redo", json!({})).unwrap();
    }

    #[test]
    fn imagegen_failure_and_bad_prompts_never_add_a_document() {
        let mut s = mock(true);
        for params in [Value::Null, json!({"prompt":true}), json!({"prompt":""}), json!({"prompt":"x".repeat(8001)}), json!({"prompt":"A mug"})] {
            assert!(s.execute("imagegen.generate", params).is_err());
            assert!(s.documents().is_empty());
        }
        let mut disabled = Session::default();
        assert!(!disabled.is_enabled("imagegen.generate"));
        assert!(disabled.execute("imagegen.generate", json!({"prompt":"A mug"})).is_err());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn imagegen_background_generation_can_be_cancelled_without_opening_a_document() {
        struct Blocking;
        impl Provider for Blocking {
            fn status(&self, _: &dyn Fn() -> bool) -> photocraft_imagegen::Result<Status> {
                Ok(Status { ready: true, ..Status::default() })
            }
            fn generate(&self, _: &str, cancelled: &dyn Fn() -> bool) -> photocraft_imagegen::Result<Vec<u8>> {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !cancelled() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err("Cancelled mock".into())
            }
        }
        let mut s = Session::default();
        s.imagegen.provider = Some(Arc::new(Blocking));
        s.execute("imagegen.status", json!({})).unwrap();
        let jobs::Started::Job(id) = s.start("imagegen.generate", json!({"prompt":"A mug"})).unwrap() else { panic!("generation must be a background job") };
        assert!(s.has_jobs());
        s.cancel_job(id);
        assert!(s.wait_job(id).is_err());
        assert!(s.documents().is_empty());
    }
}
