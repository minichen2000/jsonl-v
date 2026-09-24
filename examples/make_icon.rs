//! 生成 assets/icon.ico：cargo run --example make_icon

#[path = "../src/icon.rs"]
mod icon;

fn main() {
    let sizes = [16, 24, 32, 48, 64, 128, 256];
    let ico = icon::make_ico(&sizes);
    std::fs::create_dir_all("assets").unwrap();
    std::fs::write("assets/icon.ico", &ico).unwrap();
    println!("assets/icon.ico written ({} bytes)", ico.len());
}
