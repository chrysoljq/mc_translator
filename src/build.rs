
fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let mut res = winres::WindowsResource::new();
        res.set_icon("mc_translator.ico"); 
        res.compile().unwrap();
    }
}