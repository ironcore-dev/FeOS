// SPDX-FileCopyrightText: 2023 SAP SE or an SAP affiliate company and IronCore contributors
// SPDX-License-Identifier: Apache-2.0

use super::fixtures::{verify_vm_socket_cleanup, wait_for_target_state, VmGuard};
use super::{ensure_server, get_public_clients, skip_if_ch_binary_missing, TEST_IMAGE_REF};
use anyhow::{Context, Result};
use feos_proto::vm_service::{
    net_config, stream_vm_console_request as console_input, AttachConsoleMessage, AttachNicRequest,
    CpuConfig, CreateVmRequest, DeleteVmRequest, GetVmRequest, MemoryConfig, NetConfig,
    PauseVmRequest, PingVmRequest, RemoveNicRequest, ResumeVmRequest, ShutdownVmRequest,
    StartVmRequest, StreamVmConsoleRequest, StreamVmEventsRequest, TapConfig, VmConfig, VmState,
};
use log::info;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_create_and_start_vm() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }

    ensure_server().await;
    let (mut vm_client, _, _) = get_public_clients().await?;

    let image_ref = TEST_IMAGE_REF.clone();
    let vm_config = VmConfig {
        cpus: Some(CpuConfig {
            boot_vcpus: 2,
            max_vcpus: 2,
        }),
        memory: Some(MemoryConfig {
            size_mib: 2048,
            hugepages: false,
        }),
        image_ref,
        disks: vec![],
        net: vec![],
        ignition: None,
    };
    let create_req = CreateVmRequest {
        config: Some(vm_config),
        vm_id: None,
    };

    info!("Sending CreateVm request");
    let create_res = vm_client.create_vm(create_req).await?.into_inner();
    let vm_id = create_res.vm_id;
    info!("VM created with ID: {}", vm_id);

    let mut guard = VmGuard::new(vm_id.clone());

    info!(
        "Immediately calling StartVm for vm_id: {}, expecting error",
        &vm_id
    );
    let start_req = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    let result = vm_client.start_vm(start_req.clone()).await;
    assert!(result.is_err(), "StartVm should fail when VM is Creating");

    info!("Connecting to StreamVmEvents stream for vm_id: {}", &vm_id);
    let events_req = StreamVmEventsRequest {
        vm_id: Some(vm_id.clone()),
        ..Default::default()
    };
    let mut stream = vm_client.stream_vm_events(events_req).await?.into_inner();

    timeout(
        Duration::from_secs(180),
        wait_for_target_state(&mut stream, VmState::Created),
    )
    .await
    .expect("Timed out waiting for VM to become created")?;
    info!("VM is in CREATED state");

    info!("Sending StartVm request for vm_id: {}", &vm_id);
    vm_client.start_vm(start_req.clone()).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Running),
    )
    .await
    .expect("Timed out waiting for VM to become running")?;
    info!("VM is in RUNNING state");

    info!("Sending ShutdownVm request for vm_id: {}", &vm_id);
    let shutdown_req = ShutdownVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.shutdown_vm(shutdown_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Stopped),
    )
    .await
    .expect("Timed out waiting for VM to become stopped")?;
    info!("VM is in STOPPED state");

    info!(
        "Calling ResumeVm in Stopped state for vm_id: {}, expecting error",
        &vm_id
    );
    let resume_req = ResumeVmRequest {
        vm_id: vm_id.clone(),
    };
    let result = vm_client.resume_vm(resume_req.clone()).await;
    assert!(result.is_err(), "ResumeVm should fail when VM is Stopped");

    info!(
        "Calling StreamVmConsole in Stopped state for vm_id: {}, expecting error",
        &vm_id
    );
    let (console_tx, console_rx) = tokio::sync::mpsc::channel(1);
    let console_stream = tokio_stream::wrappers::ReceiverStream::new(console_rx);

    let attach_payload = console_input::Payload::Attach(AttachConsoleMessage {
        vm_id: vm_id.clone(),
    });
    let attach_input = StreamVmConsoleRequest {
        payload: Some(attach_payload),
    };

    console_tx
        .send(attach_input)
        .await
        .expect("Failed to send attach message");

    let response = vm_client.stream_vm_console(console_stream).await;
    assert!(
        response.is_ok(),
        "StreamVmConsole should establish stream successfully"
    );

    let mut output_stream = response.unwrap().into_inner();

    let stream_result = output_stream.next().await;
    match stream_result {
        Some(Err(status)) => {
            info!(
                "Received expected error from console stream: {}",
                status.message()
            );
            assert!(
                status.message().contains("Invalid VM state")
                    || status.message().contains("Stopped"),
                "Error should be about invalid VM state, got: {}",
                status.message()
            );
        }
        Some(Ok(_)) => {
            panic!("StreamVmConsole should fail when VM is Stopped, but got success response")
        }
        None => panic!("StreamVmConsole stream ended unexpectedly without error"),
    }

    info!("Sending StartVm request again for vm_id: {}", &vm_id);
    vm_client.start_vm(start_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Running),
    )
    .await
    .expect("Timed out waiting for VM to become running again")?;
    info!("VM is in RUNNING state again");

    info!("Sending PauseVm request for vm_id: {}", &vm_id);
    let pause_req = PauseVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.pause_vm(pause_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Paused),
    )
    .await
    .expect("Timed out waiting for VM to become paused")?;
    info!("VM is in PAUSED state");

    info!(
        "Calling StartVm in Paused state for vm_id: {}, expecting error",
        &vm_id
    );
    let start_req_paused = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    let result = vm_client.start_vm(start_req_paused).await;
    assert!(result.is_err(), "StartVm should fail when VM is Paused");

    info!("Sending ResumeVm request for vm_id: {}", &vm_id);
    vm_client.resume_vm(resume_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Running),
    )
    .await
    .expect("Timed out waiting for VM to become running after resume")?;
    info!("VM is in RUNNING state after resume");

    let get_req = GetVmRequest {
        vm_id: vm_id.clone(),
    };
    let info_res = vm_client.get_vm(get_req).await?.into_inner();
    assert_eq!(
        VmState::try_from(info_res.state).unwrap(),
        VmState::Running,
        "VM state from GetVm should be RUNNING"
    );

    info!("Pinging VMM for vm_id: {}", &vm_id);
    let ping_req = PingVmRequest {
        vm_id: vm_id.clone(),
    };
    let ping_res = vm_client.ping_vm(ping_req).await?.into_inner();
    info!("VMM Ping successful, PID: {}", ping_res.pid);
    guard.set_pid(ping_res.pid as i32);

    info!("Attaching NIC 'test-nic' to vm_id: {}", &vm_id);
    let attach_nic_req = AttachNicRequest {
        vm_id: vm_id.clone(),
        nic: Some(NetConfig {
            device_id: "test".to_string(),
            backend: Some(net_config::Backend::Tap(TapConfig {
                tap_name: "test".to_string(),
            })),
            ..Default::default()
        }),
    };
    let attach_res = vm_client.attach_nic(attach_nic_req).await?.into_inner();
    assert_eq!(attach_res.device_id, "test");
    info!(
        "AttachNic call successful, device_id: {}",
        attach_res.device_id
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    info!("Verifying NIC was attached with GetVm");
    let get_req_after_attach = GetVmRequest {
        vm_id: vm_id.clone(),
    };
    let info_res_after_attach = vm_client.get_vm(get_req_after_attach).await?.into_inner();
    let nic_found = info_res_after_attach
        .config
        .expect("VM should have a config")
        .net
        .iter()
        .any(|nic| nic.device_id == "test");
    assert!(nic_found, "Attached NIC 'test' was not found in VM config");
    info!("Successfully verified NIC attachment.");

    info!("Removing NIC 'test' from vm_id: {}", &vm_id);
    let remove_nic_req = RemoveNicRequest {
        vm_id: vm_id.clone(),
        device_id: "test".to_string(),
    };
    vm_client.remove_nic(remove_nic_req).await?;
    info!("RemoveNic call successful");

    tokio::time::sleep(Duration::from_millis(100)).await;

    info!("Verifying NIC was removed with GetVm");
    let get_req_after_remove = GetVmRequest {
        vm_id: vm_id.clone(),
    };
    let info_res_after_remove = vm_client.get_vm(get_req_after_remove).await?.into_inner();
    let nic_still_present = info_res_after_remove
        .config
        .expect("VM should have a config")
        .net
        .iter()
        .any(|nic| nic.device_id == "test");
    assert!(
        !nic_still_present,
        "Removed NIC 'test' was still found in VM config"
    );
    info!("Successfully verified NIC removal.");

    info!("Deleting VM: {}", &vm_id);
    let delete_req = DeleteVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.delete_vm(delete_req).await?.into_inner();
    info!("DeleteVm call successful");

    verify_vm_socket_cleanup(&vm_id);

    guard.disable_cleanup();
    Ok(())
}

#[tokio::test]
async fn test_vm_healthcheck_and_crash_recovery() -> Result<()> {
    if skip_if_ch_binary_missing() {
        return Ok(());
    }

    ensure_server().await;
    let (mut vm_client, _, _) = get_public_clients().await?;

    let image_ref = TEST_IMAGE_REF.clone();
    let vm_config = VmConfig {
        cpus: Some(CpuConfig {
            boot_vcpus: 1,
            max_vcpus: 1,
        }),
        memory: Some(MemoryConfig {
            size_mib: 1024,
            hugepages: false,
        }),
        image_ref,
        disks: vec![],
        net: vec![],
        ignition: None,
    };
    let create_req = CreateVmRequest {
        config: Some(vm_config),
        vm_id: None,
    };

    info!("Sending CreateVm request for healthcheck test");
    let create_res = vm_client.create_vm(create_req).await?.into_inner();
    let vm_id = create_res.vm_id;
    info!("VM created with ID: {}", vm_id);

    let mut guard = VmGuard::new(vm_id.clone());

    info!("Connecting to StreamVmEvents stream for vm_id: {}", &vm_id);
    let events_req = StreamVmEventsRequest {
        vm_id: Some(vm_id.clone()),
        ..Default::default()
    };
    let mut stream = vm_client.stream_vm_events(events_req).await?.into_inner();

    timeout(
        Duration::from_secs(180),
        wait_for_target_state(&mut stream, VmState::Created),
    )
    .await
    .expect("Timed out waiting for VM to become created")?;
    info!("VM is in CREATED state");

    info!("Sending StartVm request for vm_id: {}", &vm_id);
    let start_req = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.start_vm(start_req).await?;

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Running),
    )
    .await
    .expect("Timed out waiting for VM to become running")?;
    info!("VM is in RUNNING state");

    info!("Pinging VMM for vm_id: {}", &vm_id);
    let ping_req = PingVmRequest {
        vm_id: vm_id.clone(),
    };
    let ping_res = vm_client.ping_vm(ping_req).await?.into_inner();
    info!("VMM Ping successful, PID: {}", ping_res.pid);
    let pid_to_kill = Pid::from_raw(ping_res.pid as i32);
    guard.set_pid(ping_res.pid as i32);

    info!(
        "Forcefully killing hypervisor process with PID: {}",
        pid_to_kill
    );
    kill(pid_to_kill, Signal::SIGKILL).context("Failed to kill hypervisor process")?;
    info!("Successfully sent SIGKILL to process {}", pid_to_kill);

    timeout(
        Duration::from_secs(30),
        wait_for_target_state(&mut stream, VmState::Crashed),
    )
    .await
    .expect("Timed out waiting for VM to enter Crashed state")?;
    info!("VM is in CRASHED state as expected");

    info!("Verifying API calls fail for crashed VM: {}", &vm_id);

    let start_req = StartVmRequest {
        vm_id: vm_id.clone(),
    };
    assert!(
        vm_client.start_vm(start_req).await.is_err(),
        "StartVm should fail for a crashed VM"
    );

    let pause_req = PauseVmRequest {
        vm_id: vm_id.clone(),
    };
    assert!(
        vm_client.pause_vm(pause_req).await.is_err(),
        "PauseVm should fail for a crashed VM"
    );

    let shutdown_req = ShutdownVmRequest {
        vm_id: vm_id.clone(),
    };
    assert!(
        vm_client.shutdown_vm(shutdown_req).await.is_err(),
        "ShutdownVm should fail for a crashed VM"
    );

    let resume_req = ResumeVmRequest {
        vm_id: vm_id.clone(),
    };
    assert!(
        vm_client.resume_vm(resume_req).await.is_err(),
        "ResumeVm should fail for a crashed VM"
    );

    info!("API call failure checks passed for crashed VM");

    info!("Deleting crashed VM: {}", &vm_id);
    let delete_req = DeleteVmRequest {
        vm_id: vm_id.clone(),
    };
    vm_client.delete_vm(delete_req).await?.into_inner();
    info!("DeleteVm call successful for crashed VM");

    verify_vm_socket_cleanup(&vm_id);

    guard.disable_cleanup();
    Ok(())
}
