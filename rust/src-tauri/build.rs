use std::process::Command;

fn main() {
    // 构建时嵌入的元数据（运行时通过 env! 读取）
    emit_env("VC_GIT_HASH", git_short_hash());
    emit_env("VC_GIT_DIRTY", git_dirty_flag());
    emit_env("VC_RUSTC_VERSION", rustc_version());
    emit_env("VC_BUILD_TIME", chrono::Utc::now().to_rfc3339());
    emit_env("VC_TARGET", std::env::var("TARGET").unwrap_or_default());

    // sherpa-onnx 在 shared 模式下 link libsherpa-onnx-c-api.dylib + libonnxruntime.dylib;
    // 这两个 dylib 由 tauri.conf.json 的 bundle.macOS.files 复制进 .app/Contents/Frameworks/。
    // 这里给主二进制加上 LC_RPATH 指向 Frameworks/,运行时 dyld 才找得到。
    // sherpa-onnx-sys 自己 build.rs 里也加了 rpath link arg,但 cargo link arg 不会传递
    // 到上游 crate(就是这个 binary),所以这条必须在 voice-claude 自己的 build.rs 里加。
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }

    tauri_build::build()
}

fn emit_env(key: &str, val: String) {
    println!("cargo:rustc-env={}={}", key, val);
    println!("cargo:rerun-if-env-changed={}", key);
}

fn git_short_hash() -> String {
    Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into())
}

fn git_dirty_flag() -> String {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|out| {
            if out.stdout.is_empty() {
                "clean".into()
            } else {
                "dirty".into()
            }
        })
        .unwrap_or_else(|| "unknown".into())
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into())
}
