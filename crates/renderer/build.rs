fn main() {
    #[cfg(target_os = "windows")]
    {
        // Embed the app icon so the exe, taskbar entries and the tray (which
        // extracts the first icon of its own module) all show the brand. The
        // version resource says who and what the program is: Task Manager and
        // the file's Properties show these, and a signing service checks them.
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/icon/limekit.ico");
        resource.set("ProductName", "LimeKit");
        resource.set("FileDescription", "LimeKit wallpaper player");
        resource.set("CompanyName", "2fame");
        resource.set("LegalCopyright", "2fame");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=failed to embed the icon: {error}");
        }
        println!("cargo:rerun-if-changed=../../assets/icon/limekit.ico");
    }
}
