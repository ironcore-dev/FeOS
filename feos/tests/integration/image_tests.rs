// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::{ensure_server, get_image_service_client, skip_if_ch_binary_missing, TEST_IMAGE_REF};
use anyhow::Result;
use feos_proto::image_service::{
    DeleteImageRequest, ImageState, ImageStatusResponse, ListImagesRequest, PullImageRequest,
    WatchImageStatusRequest,
};
use image_service::IMAGE_DIR;
use log::info;
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::Status;

#[tokio::test]
async fn test_image_lifecycle() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }
    ensure_server().await;
    let mut image_client = get_image_service_client().await?;

    let image_ref = TEST_IMAGE_REF.clone();
    info!("Pulling image: {}", image_ref);
    let pull_req = PullImageRequest {
        image_ref: image_ref.clone(),
    };
    let pull_res = image_client.pull_image(pull_req).await?.into_inner();
    let image_uuid = pull_res.image_uuid;
    info!("Image pull initiated with UUID: {}", image_uuid);

    let watch_req = WatchImageStatusRequest {
        image_uuid: image_uuid.clone(),
    };
    let mut stream = image_client
        .watch_image_status(watch_req)
        .await?
        .into_inner();

    timeout(Duration::from_secs(120), wait_for_image_ready(&mut stream))
        .await
        .expect("Timed out waiting for image to become ready")?;

    info!("Verifying image {} is in the list...", image_uuid);
    let list_req = ListImagesRequest {};
    let list_res = image_client.list_images(list_req).await?.into_inner();
    let found_image = list_res
        .images
        .iter()
        .find(|i| i.image_uuid == image_uuid)
        .expect("Image UUID should be in the list after pulling");
    assert_eq!(found_image.state, ImageState::Ready as i32);

    let image_path = Path::new(IMAGE_DIR).join(&image_uuid);
    info!("Verifying filesystem path: {}", image_path.display());
    assert!(image_path.exists(), "Image directory should exist");
    assert!(image_path.join("disk.image").exists());
    assert!(image_path.join("metadata.json").exists());

    info!("Deleting image: {}", image_uuid);
    let delete_req = DeleteImageRequest {
        image_uuid: image_uuid.clone(),
    };
    image_client.delete_image(delete_req).await?;

    info!("Verifying image {} is NOT in the list...", image_uuid);
    let list_req_after_delete = ListImagesRequest {};
    let list_res_after_delete = image_client
        .list_images(list_req_after_delete)
        .await?
        .into_inner();
    assert!(!list_res_after_delete
        .images
        .iter()
        .any(|i| i.image_uuid == image_uuid));

    info!(
        "Verifying filesystem path is gone: {}",
        image_path.display()
    );
    assert!(!image_path.exists(), "Image directory should be deleted");

    Ok(())
}

#[tokio::test]
async fn test_container_image_lifecycle() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }
    ensure_server().await;
    let mut image_client = get_image_service_client().await?;

    let image_ref = "ghcr.io:5000/appvia/hello-world/hello-world".to_string();
    info!("Pulling container image: {}", image_ref);
    let pull_req = PullImageRequest {
        image_ref: image_ref.clone(),
    };
    let pull_res = image_client.pull_image(pull_req).await?.into_inner();
    let image_uuid = pull_res.image_uuid;
    info!("Container image pull initiated with UUID: {}", image_uuid);

    let watch_req = WatchImageStatusRequest {
        image_uuid: image_uuid.clone(),
    };
    let mut stream = image_client
        .watch_image_status(watch_req)
        .await?
        .into_inner();

    timeout(Duration::from_secs(120), wait_for_image_ready(&mut stream))
        .await
        .expect("Timed out waiting for container image to become ready")?;

    info!("Verifying container image {} is in the list...", image_uuid);
    let list_req = ListImagesRequest {};
    let list_res = image_client.list_images(list_req).await?.into_inner();
    let found_image = list_res
        .images
        .iter()
        .find(|i| i.image_uuid == image_uuid)
        .expect("Image UUID should be in the list after pulling");
    assert_eq!(found_image.state, ImageState::Ready as i32);

    let image_path = Path::new(IMAGE_DIR).join(&image_uuid);
    info!(
        "Verifying container filesystem path: {}",
        image_path.display()
    );
    assert!(image_path.exists(), "Image directory should exist");
    assert!(
        image_path.join("rootfs").exists(),
        "rootfs directory should exist for container image"
    );
    assert!(
        image_path.join("config.json").exists(),
        "config.json should exist for container image"
    );
    assert!(image_path.join("metadata.json").exists());

    info!("Deleting container image: {}", image_uuid);
    let delete_req = DeleteImageRequest {
        image_uuid: image_uuid.clone(),
    };
    image_client.delete_image(delete_req).await?;

    info!(
        "Verifying container image {} is NOT in the list...",
        image_uuid
    );
    let list_req_after_delete = ListImagesRequest {};
    let list_res_after_delete = image_client
        .list_images(list_req_after_delete)
        .await?
        .into_inner();
    assert!(!list_res_after_delete
        .images
        .iter()
        .any(|i| i.image_uuid == image_uuid));

    info!(
        "Verifying filesystem path is gone: {}",
        image_path.display()
    );
    assert!(!image_path.exists(), "Image directory should be deleted");

    Ok(())
}

async fn wait_for_image_ready<S>(mut stream: S) -> anyhow::Result<()>
where
    S: tokio_stream::Stream<Item = Result<ImageStatusResponse, Status>> + Unpin,
{
    let mut saw_downloading = false;
    while let Some(status_res) = stream.next().await {
        let status = status_res?;
        let state = ImageState::try_from(status.state).unwrap();
        info!("Received image status update: {:?}", state);
        if state == ImageState::Downloading {
            saw_downloading = true;
        }
        if state == ImageState::Ready {
            assert!(
                saw_downloading,
                "Should have seen DOWNLOADING state before READY"
            );
            return Ok(());
        }
        if state == ImageState::PullFailed {
            panic!("Image pull failed unexpectedly: {}", status.message);
        }
    }
    anyhow::bail!("Stream ended before image became ready");
}
