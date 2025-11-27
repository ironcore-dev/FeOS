// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    ensure_server, get_public_clients, skip_if_youki_binary_missing, TEST_CONTAINER_IMAGE_REF,
};
use anyhow::Result;
use feos_proto::container_service::{
    ContainerConfig, ContainerState, CreateContainerRequest, DeleteContainerRequest,
    GetContainerRequest, StartContainerRequest, StopContainerRequest,
};
use log::info;
use std::time::Duration;
use tokio::time::timeout;

async fn wait_for_container_state(
    client: &mut feos_proto::container_service::container_service_client::ContainerServiceClient<
        tonic::transport::Channel,
    >,
    container_id: &str,
    target_state: ContainerState,
) -> Result<()> {
    info!("Waiting for container {container_id} to reach state {target_state:?}");
    for _ in 0..180 {
        let response = client
            .get_container(GetContainerRequest {
                container_id: container_id.to_string(),
            })
            .await?
            .into_inner();

        let current_state = ContainerState::try_from(response.state).unwrap_or_default();
        info!("Container {container_id} is in state {current_state:?}");

        if current_state == target_state {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    anyhow::bail!("Timeout waiting for container {container_id} to reach state {target_state:?}");
}

#[tokio::test]
async fn test_create_and_start_container() -> Result<()> {
    if skip_if_youki_binary_missing() {
        return Ok(());
    }
    ensure_server().await;
    let (_, _, mut container_client) = get_public_clients().await?;

    let image_ref = TEST_CONTAINER_IMAGE_REF.clone();
    let container_config = ContainerConfig {
        image_ref,
        command: vec![],
        env: Default::default(),
    };

    let create_req = CreateContainerRequest {
        config: Some(container_config),
        container_id: None,
    };

    info!("Sending CreateContainer request");
    let create_res = container_client
        .create_container(create_req)
        .await?
        .into_inner();
    let container_id = create_res.container_id;
    info!("Container creation initiated with ID: {container_id}");

    timeout(
        Duration::from_secs(180),
        wait_for_container_state(
            &mut container_client,
            &container_id,
            ContainerState::Created,
        ),
    )
    .await
    .expect("Timed out waiting for container to become created")?;
    info!("Container is in CREATED state");

    let start_req = StartContainerRequest {
        container_id: container_id.clone(),
    };
    info!("Sending StartContainer request for ID: {container_id}");
    container_client.start_container(start_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_container_state(
            &mut container_client,
            &container_id,
            ContainerState::Running,
        ),
    )
    .await
    .expect("Timed out waiting for container to become running")?;
    info!("Container is in RUNNING state");

    let stop_req = StopContainerRequest {
        container_id: container_id.clone(),
        signal: None,
        timeout_seconds: None,
    };
    info!("Sending StopContainer request for ID: {container_id}");
    container_client.stop_container(stop_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_container_state(
            &mut container_client,
            &container_id,
            ContainerState::Stopped,
        ),
    )
    .await
    .expect("Timed out waiting for container to become stopped")?;
    info!("Container is in STOPPED state");

    let delete_req = DeleteContainerRequest {
        container_id: container_id.clone(),
    };
    info!("Sending DeleteContainer request for ID: {container_id}");
    container_client.delete_container(delete_req).await?;
    info!("DeleteContainer call successful");

    let get_req = GetContainerRequest {
        container_id: container_id.clone(),
    };
    let result = container_client.get_container(get_req).await;
    assert!(result.is_err(), "GetContainer should fail after deletion");
    info!("Verified that container {container_id} is no longer found.");

    Ok(())
}
