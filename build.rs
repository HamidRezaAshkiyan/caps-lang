fn main() {
    println!("cargo:rerun-if-changed=assets/caps-lang.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/caps-lang.ico");
        res.set("ProductName", "CapsLang");
        res.set(
            "FileDescription",
            "macOS-style Caps Lock for Windows (Tap: Language, Hold: Caps Lock)",
        );
        res.set("CompanyName", "HamidRezaAshkiyan");
        res.compile().expect("failed to compile Windows resources");
    }
}
