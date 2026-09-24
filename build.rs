fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/icon.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if !std::path::Path::new("assets/icon.ico").exists() {
        // icon.ico 由 `cargo run --example make_icon` 生成；未生成时跳过内嵌
        println!("cargo:warning=assets/icon.ico 不存在，跳过 exe 图标内嵌");
        return;
    }
    embed_resource::compile("assets/icon.rc", embed_resource::NONE);
}
