use crate::downloader;
use crate::models::DownloadProgress;
use std::path::Path;
use tauri::{AppHandle, Emitter};
use tokio::fs;

pub async fn unzip_all(dest_dir: &str, app: &AppHandle) -> Result<(), String> {
    let base = Path::new(dest_dir);
    // Unzip webapp files -> static-file-service/
    let webapp_dir = base.join("webapp");
    if webapp_dir.exists() && !downloader::is_cancelled() {
        let target = base.join("static-file-service");
        let _ = fs::create_dir_all(&target).await;
        unzip_dir(&webapp_dir, &target, base, app).await?;
    }

    // Unzip appstore subdirs -> mservice-cosmic/lib/{subdir}/
    let appstore_dir = base.join("appstore");
    if appstore_dir.exists() && !downloader::is_cancelled() {
        for subdir in &["bos", "biz", "trd", "cus"] {
            if downloader::is_cancelled() {
                break;
            }
            let sub_path = appstore_dir.join(subdir);
            if sub_path.exists() {
                let target = base
                    .join("mservice-cosmic")
                    .join("lib")
                    .join(subdir);
                let _ = fs::create_dir_all(&target).await;
                unzip_dir(&sub_path, &target, base, app).await?;
            }
        }
    }

    Ok(())
}

async fn unzip_dir(
    src_dir: &Path,
    dest_dir: &Path,
    base_dir: &Path,
    app: &AppHandle,
) -> Result<(), String> {
    let mut entries = fs::read_dir(src_dir)
        .await
        .map_err(|e| format!("读取目录失败: {}", e))?;

    let mut zip_files = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "zip") {
            zip_files.push(path);
        }
    }

    let total = zip_files.len();
    for (i, zip_path) in zip_files.iter().enumerate() {
        if downloader::is_cancelled() {
            let name = zip_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let rel_path = zip_path.strip_prefix(base_dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            emit_progress(app, &name, &rel_path, "pending", 0);
            continue;
        }

        let name = zip_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let rel_path = zip_path.strip_prefix(base_dir)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();

        emit_progress(app, &name, &rel_path, "unzipping", 0);

        let zip_path_owned = zip_path.clone();
        let dest_dir_owned = dest_dir.to_path_buf();
        let name_owned = name.clone();
        let rel_path_owned = rel_path.clone();

        tokio::task::spawn_blocking(move || {
            unzip_file(&zip_path_owned, &dest_dir_owned)
        })
        .await
        .map_err(|e| format!("解压任务失败: {}", e))?
        .map_err(|e| format!("解压 {} 失败: {}", name, e))?;

        let percent = if total > 0 {
            (((i + 1) as f64 / total as f64) * 100.0) as u32
        } else {
            100
        };
        emit_progress(app, &name_owned, &rel_path_owned, "unzipped", percent);
    }

    Ok(())
}

fn unzip_file(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("打开zip失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("读取zip失败: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取zip条目失败: {}", e))?;

        let out_path = dest_dir.join(entry.mangled_name());

        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目录失败: {}", e))?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("创建文件失败: {}", e))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("写入文件失败: {}", e))?;
        }
    }

    Ok(())
}

fn emit_progress(app: &AppHandle, filename: &str, local_path: &str, status: &str, progress: u32) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            filename: filename.to_string(),
            local_path: local_path.to_string(),
            status: status.to_string(),
            progress,
            downloaded_bytes: 0,
            total_bytes: 0,
        },
    );
}
