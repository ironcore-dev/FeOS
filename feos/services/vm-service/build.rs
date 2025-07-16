fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-env=SQLX_OFFLINE=true");
    Ok(())
}
