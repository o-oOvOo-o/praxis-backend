#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let praxis_home = praxis_utils_home_dir::find_praxis_home()?;
    praxis_login::login_anthropic_oauth(&praxis_home).await?;
    println!("Anthropic OAuth login completed.");
    Ok(())
}
