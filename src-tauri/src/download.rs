use crate::{
    errors::ModelDownloadError,
    signal,
    state::{AppState, Options, MODELS},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use tauri::{ipc::Channel, AppHandle};
use tokio::io::AsyncWriteExt;

const TEXT_DETECTION_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten";
const TEXT_RECOGNITION_URL: &str =
    "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten";
const TEXT_DETECTION_HASH: [u8; 32] =
    hex_literal::hex!("f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca");
const TEXT_RECOGNITION_HASH: [u8; 32] =
    hex_literal::hex!("e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e");
pub async fn get_or_download_models(
    app: AppHandle,
    frontend_channel: Channel<ModelDownload>,
) -> Result<(), ModelDownloadError> {
    let Options { ocr } = AppState::get_options(&app);
    if !ocr {
        return Ok(());
    }

    let mut cache_dir = dirs::cache_dir().ok_or(ModelDownloadError::CacheDirUnknown)?;
    cache_dir.push("quikscore");

    let detection_model = cache_dir.join("text-detection.rten");
    let recognition_model = cache_dir.join("text-recognition.rten");

    let detection_model_exists = detection_model.try_exists()?;
    let recognition_model_exists = recognition_model.try_exists()?;

    let mut hasher = Sha256::new();
    let need_download_detection = if detection_model_exists {
        let mut detection_model_file = File::open(&detection_model)?;
        _ = std::io::copy(&mut detection_model_file, &mut hasher)?;
        let hash = hasher.finalize_reset();
        let hash_not_passed = hash[..] != TEXT_DETECTION_HASH[..];
        if hash_not_passed {
            println!("detection model hash mismatch, redownloading");
        }
        hash_not_passed
    } else {
        println!("downloading detection model");
        true
    };
    let need_download_recognition = if recognition_model_exists {
        let mut recognition_model_file = File::open(&recognition_model)?;
        _ = std::io::copy(&mut recognition_model_file, &mut hasher)?;
        let hash_not_passed = hasher.finalize()[..] != TEXT_RECOGNITION_HASH[..];
        if hash_not_passed {
            println!("recognition model hash mismatch, redownloading")
        }
        hash_not_passed
    } else {
        println!("downloading recognition model");
        true
    };

    if need_download_detection || need_download_recognition {
        #[derive(Debug)]
        enum Progress {
            Detection { downloaded: u32, total: u32 },
            DetectionDone,
            Recognition { downloaded: u32, total: u32 },
            RecognitionDone,
        }
        let (tx, mut rx) =
            tauri::async_runtime::channel::<Result<Progress, ModelDownloadError>>(1024);
        let (mut progress_detection, mut progress_recognition, mut totals) =
            (0u32, 0u32, [0u32, 0u32]);

        let client = reqwest::Client::new();
        if need_download_detection {
            let client = client.clone();
            let tx = tx.clone();
            tauri::async_runtime::spawn(async move {
                let resp_head = match client.head(TEXT_DETECTION_URL).send().await {
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

                let resp = match client.get(TEXT_DETECTION_URL).send().await {
                    Ok(it) => it,
                    Err(err) => {
                        _ = tx.send(Err(err.into())).await;
                        return;
                    }
                };
                let mut file = match tokio::fs::File::create(detection_model).await {
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
                    let _ = tx.send(Ok(Progress::Detection { downloaded, total })).await;
                }
                let _ = tx.send(Ok(Progress::DetectionDone)).await;
            });
        }
        if need_download_recognition {
            let client = client.clone();
            let tx = tx.clone();
            tauri::async_runtime::spawn(async move {
                let resp_head = match client.head(TEXT_RECOGNITION_URL).send().await {
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

                let resp = match client.get(TEXT_RECOGNITION_URL).send().await {
                    Ok(it) => it,
                    Err(err) => {
                        _ = tx.send(Err(err.into())).await;
                        return;
                    }
                };
                let mut file = match tokio::fs::File::create(recognition_model).await {
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
                    let _ = tx
                        .send(Ok(Progress::Recognition { downloaded, total }))
                        .await;
                }
                let _ = tx.send(Ok(Progress::RecognitionDone)).await;
            });
        }

        let (mut detection_finished, mut recognition_finished) = (false, false);
        while let Some(p) = rx.recv().await {
            match p {
                Ok(Progress::Detection { downloaded, total }) => {
                    progress_detection = downloaded;
                    totals[0] = total;
                }
                Ok(Progress::Recognition { downloaded, total }) => {
                    progress_recognition = downloaded;
                    totals[1] = total;
                }
                Ok(p) => {
                    match p {
                        Progress::DetectionDone => detection_finished = true,
                        Progress::RecognitionDone => recognition_finished = true,
                        _ => unreachable!(),
                    }
                    let close = !need_download_detection
                        && need_download_recognition
                        && recognition_finished
                        || need_download_detection
                            && !need_download_recognition
                            && detection_finished
                        || need_download_detection
                            && need_download_recognition
                            && detection_finished
                            && recognition_finished;
                    if close {
                        rx.close();
                    }
                }
                Err(e) => return Err(e),
            }
            signal!(
                frontend_channel,
                ModelDownload::Progress {
                    progress_detection,
                    progress_recognition,
                    total: totals.iter().sum()
                }
            )
        }
        signal!(frontend_channel, ModelDownload::Success);
        println!("download success");
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
    Progress {
        progress_detection: u32,
        progress_recognition: u32,
        total: u32,
    },
    Success,
}
