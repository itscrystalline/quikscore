use crate::err_log;
use crate::{
    errors::ModelDownloadError,
    signal,
    state::{AppState, Options, MODELS},
};
use futures::StreamExt;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use tauri::{ipc::Channel, AppHandle};
use tokio::io::AsyncWriteExt;

const ENG_TESSDATA: &str =
    "https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/refs/heads/main/eng.traineddata";
const ENG_TESSDATA_HASH: [u8; 32] =
    hex_literal::hex!("8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba");

pub async fn get_or_download_models(
    app: AppHandle,
    frontend_channel: Channel<ModelDownload>,
) -> Result<(), ModelDownloadError> {
    let Options { ocr, mongo: _ } = AppState::get_options(&app);
    if !ocr {
        return Ok(());
    }

    let mut cache_dir = dirs::cache_dir().ok_or(ModelDownloadError::CacheDirUnknown)?;
    cache_dir.push("quikscore");

    let tessdata = cache_dir.join("eng.traineddata");

    let tessdata_exists = tessdata.try_exists()?;

    let mut hasher = Sha256::new();
    let need_download_tessdata = if tessdata_exists {
        let mut tessdata_file = File::open(&tessdata)?;
        _ = std::io::copy(&mut tessdata_file, &mut hasher)?;
        let hash = hasher.finalize_reset();
        let hash_not_passed = hash[..] != ENG_TESSDATA_HASH[..];
        if hash_not_passed {
            warn!("Tesseract data hash mismatch, redownloading");
        }
        hash_not_passed
    } else {
        info!("Downloading Tesseract data...");
        true
    };

    if need_download_tessdata {
        #[derive(Debug)]
        enum Progress {
            Progress { downloaded: u32, total: u32 },
            Done,
        }
        let (tx, mut rx) =
            tauri::async_runtime::channel::<Result<Progress, ModelDownloadError>>(1024);
        let (mut progress, mut totals) = (0u32, 0u32);

        let client = reqwest::Client::new();
        let tx = tx.clone();
        tauri::async_runtime::spawn(async move {
            let resp_head = match client.head(ENG_TESSDATA).send().await {
                Ok(it) => it,
                Err(err) => {
                    _ = tx.send(Err(err.into())).await;
                    return;
                }
            };
            let total = match resp_head
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .ok_or(ModelDownloadError::NoContentLength)
                .and_then(|h| Ok(h.to_str()?))
                .and_then(|s| Ok(s.parse::<u32>()?))
            {
                Ok(it) => it,
                Err(err) => {
                    _ = tx.send(Err(err)).await;
                    return;
                }
            };

            let resp = match client.get(ENG_TESSDATA).send().await {
                Ok(it) => it,
                Err(err) => {
                    _ = tx.send(Err(err.into())).await;
                    return;
                }
            };
            let mut file = match tokio::fs::File::create(tessdata).await {
                Ok(it) => it,
                Err(err) => {
                    _ = tx.send(Err(err.into())).await;
                    return;
                }
            };
            let mut stream = resp.bytes_stream();

            let mut downloaded = 0u32;

            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(it) => it,
                    Err(err) => {
                        _ = tx.send(Err(err.into())).await;
                        return;
                    }
                };
                if let Err(err) = file.write_all(&chunk).await {
                    _ = tx.send(Err(err.into())).await;
                    return;
                }
                downloaded += chunk.len() as u32;

                // send progress update (ignoring send errors if receiver closed)
                let _ = tx.send(Ok(Progress::Progress { downloaded, total })).await;
            }
            let _ = tx.send(Ok(Progress::Done)).await;
        });

        while let Some(p) = rx.recv().await {
            match p {
                Ok(Progress::Progress { downloaded, total }) => {
                    progress = downloaded;
                    totals = total;
                }
                Ok(Progress::Done) => rx.close(),
                Err(e) => {
                    err_log!(&e);
                    return Err(e);
                }
            }
            signal!(
                frontend_channel,
                ModelDownload::Progress {
                    progress,
                    total: totals
                }
            )
        }
        signal!(frontend_channel, ModelDownload::Success);
        info!("Download success!");
    }

    _ = MODELS.set(cache_dir);
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum ModelDownload {
    Progress { progress: u32, total: u32 },
    Success,
}
