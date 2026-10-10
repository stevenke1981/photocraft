//! Read only public Codex connection metadata; never generate an image or start a login.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), String> {
    use photocraft_imagegen::{CodexProvider, Provider};
    let provider = CodexProvider::discover().ok_or("Official Codex CLI not found")?;
    let status = provider.status(&|| false)?;
    println!("{}", serde_json::to_string_pretty(&status).map_err(|e| e.to_string())?);
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {}
