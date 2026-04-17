fn main() {
    let sdr_arch = if cfg!(target_pointer_width = "64") { "x64" } else { "x86" };

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    // speakplain/src-tauri/ -> speakplain/ -> sdr/
    let sdr_dir = std::path::Path::new(&manifest_dir)
        .parent()  // speakplain/
        .unwrap()
        .join("sdr")
        .join(sdr_arch);

    // .exe 目录： OUT_DIR 上溯三级
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let exe_dir = std::path::Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();

    // 复制 rtl_sdr.exe + rtl_test.exe 到输出目录（运行时通过子进程调用）
    for exe in &["rtl_sdr.exe", "rtl_test.exe"] {
        let src = sdr_dir.join(exe);
        let dst = exe_dir.join(exe);
        if src.exists() {
            std::fs::copy(&src, &dst).ok();
        }
    }

    // 复制运行时 DLL（rtl_tcp.exe 自身依赖）
    for dll in &["rtlsdr.dll", "pthreadVC2.dll", "msvcr100.dll"] {
        let src = sdr_dir.join(dll);
        let dst = exe_dir.join(dll);
        if src.exists() {
            std::fs::copy(&src, &dst).ok();
        }
    }

    // 复制 Zadig（首次安装驱动）
    let zadig_src = std::path::Path::new(&manifest_dir)
        .parent().unwrap()   // speakplain/
        .join("sdr")
        .join("zadig-2.9.exe");
    let zadig_dst = exe_dir.join("zadig.exe");
    if zadig_src.exists() && !zadig_dst.exists() {
        std::fs::copy(&zadig_src, &zadig_dst).ok();
    }

    tauri_build::build()
}
