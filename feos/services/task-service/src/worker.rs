use crate::error::TaskError;
use crate::Event;
use feos_proto::task_service::{
    CreateRequest, CreateResponse, DeleteRequest, DeleteResponse, KillRequest, KillResponse,
    StartRequest, StartResponse,
};
use log::{debug, error, info, warn};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};

const YOUKI_BIN: &str = "youki";

/// Executes a short-lived youki command (like start, kill, delete) and waits for it to complete.
/// This version is safe as these commands are guaranteed to be short-lived.
async fn run_youki_command(args: &[&str]) -> Result<(), TaskError> {
    info!(
        "Worker: Executing short-lived command: {} {}",
        YOUKI_BIN,
        args.join(" ")
    );

    let output = Command::new(YOUKI_BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| TaskError::YoukiCommand(format!("Failed to execute youki process: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let err_msg = format!(
            "youki exited with code {}: stderr='{}', stdout='{}'",
            output.status, stderr, stdout
        );
        error!("Worker: {err_msg}");
        return Err(TaskError::YoukiCommand(err_msg));
    }

    debug!("Worker: Youki command successful.");
    Ok(())
}

/// Creates a container using the spawn method to avoid hangs, waits for the launcher to exit,
/// and then gets the true container PID from the pid-file.
pub async fn handle_create(
    req: CreateRequest,
    event_tx: mpsc::Sender<Event>,
    responder: oneshot::Sender<Result<CreateResponse, TaskError>>,
) {
    let id = req.container_id.clone();
    let pid_file = format!("{}/container.pid", req.bundle_path);

    let args = &[
        "create",
        "--bundle",
        &req.bundle_path,
        "--pid-file",
        &pid_file,
        &id,
    ];

    info!(
        "Worker: Spawning youki create command: {} {}",
        YOUKI_BIN,
        args.join(" ")
    );

    // Use spawn() to avoid potential I/O deadlocks that hang the command.
    let child_result = Command::new(YOUKI_BIN)
        .args(args)
        // Redirect stdio to null to ensure the youki process does not block on I/O.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let mut child = match child_result {
        Ok(child) => child,
        Err(e) => {
            let err = TaskError::YoukiCommand(format!("Failed to spawn youki create: {e}"));
            let _ = event_tx
                .send(Event::ContainerCreateFailed {
                    id,
                    error: err.clone(),
                })
                .await;
            let _ = responder.send(Err(err));
            return;
        }
    };

    // Now, wait for the short-lived `youki create` process to exit.
    // This is non-blocking and safe because we are not tied to its I/O pipes.
    let status = match child.wait().await {
        Ok(status) => status,
        Err(e) => {
            let err =
                TaskError::YoukiCommand(format!("Failed to wait for youki create process: {e}"));
            let _ = event_tx
                .send(Event::ContainerCreateFailed {
                    id,
                    error: err.clone(),
                })
                .await;
            let _ = responder.send(Err(err));
            return;
        }
    };

    if !status.success() {
        // Since we redirected output, we can't show it here, but we can report the failure.
        let err = TaskError::YoukiCommand(format!(
            "youki create exited with non-zero status: {status}"
        ));
        let _ = event_tx
            .send(Event::ContainerCreateFailed {
                id,
                error: err.clone(),
            })
            .await;
        let _ = responder.send(Err(err));
        return;
    }

    // Now that `youki create` has exited, read the PID of the real container process.
    let result: Result<i32, TaskError> = async {
        let pid_str = tokio::fs::read_to_string(&pid_file)
            .await
            .map_err(|e| TaskError::Internal(format!("Could not read pid file: {e}")))?;
        let pid = pid_str
            .trim()
            .parse::<i32>()
            .map_err(|e| TaskError::Internal(format!("Failed to parse PID from file: {e}")))?;
        tokio::fs::remove_file(&pid_file)
            .await
            .map_err(|e| TaskError::Internal(format!("Could not remove pid file: {e}")))?;
        Ok(pid)
    }
    .await;

    match result {
        Ok(pid) => {
            info!("Worker: Got actual container PID {pid} for '{id}' from pid-file");
            let _ = event_tx.send(Event::ContainerCreated { id, pid }).await;
            let _ = responder.send(Ok(CreateResponse { pid: pid as i64 }));
        }
        Err(e) => {
            let _ = event_tx
                .send(Event::ContainerCreateFailed {
                    id,
                    error: e.clone(),
                })
                .await;
            let _ = responder.send(Err(e));
        }
    }
}

/// Starts a previously created container.
pub async fn handle_start(
    req: StartRequest,
    pid: i32,
    event_tx: mpsc::Sender<Event>,
    responder: oneshot::Sender<Result<StartResponse, TaskError>>,
) {
    let id = req.container_id.clone();
    let result = run_youki_command(&["start", &id]).await;

    match result {
        Ok(_) => {
            let _ = event_tx
                .send(Event::ContainerStarted { id: id.clone() })
                .await;
            let _ = responder.send(Ok(StartResponse {}));
            tokio::spawn(wait_for_process_exit(id, pid, event_tx));
        }
        Err(e) => {
            let _ = event_tx
                .send(Event::ContainerStartFailed {
                    id,
                    error: e.clone(),
                })
                .await;
            let _ = responder.send(Err(e));
        }
    }
}

/// Sends a signal to the container's init process.
pub async fn handle_kill(
    req: KillRequest,
    responder: oneshot::Sender<Result<KillResponse, TaskError>>,
) {
    let signal = req.signal.to_string();
    let result = run_youki_command(&["kill", &req.container_id, &signal]).await;
    let _ = responder.send(result.map(|_| KillResponse {}));
}

/// Deletes the container's resources.
pub async fn handle_delete(
    req: DeleteRequest,
    event_tx: mpsc::Sender<Event>,
    responder: oneshot::Sender<Result<DeleteResponse, TaskError>>,
) {
    let id = req.container_id.clone();
    let result = run_youki_command(&["delete", "--force", &id]).await;

    if let Err(e) = result {
        let _ = responder.send(Err(e));
        return;
    }

    let _ = event_tx.send(Event::ContainerDeleted { id }).await;
    let _ = responder.send(Ok(DeleteResponse {}));
}

/// Waits in the background for a container process to exit and sends an event.
pub async fn wait_for_process_exit(id: String, pid: i32, event_tx: mpsc::Sender<Event>) {
    info!("Worker: Background task started, waiting for PID {pid} ({id}) to exit");
    let pid_obj = Pid::from_raw(pid);

    let wait_result = waitpid(pid_obj, None);

    let status = match wait_result {
        Ok(status) => status,
        Err(e) => {
            error!("Worker: waitpid failed for PID {pid}: {e}");
            return;
        }
    };

    let exit_code = match status {
        WaitStatus::Exited(_, code) => {
            info!("Worker: Process {pid} ({id}) exited with code {code}");
            code
        }
        WaitStatus::Signaled(_, signal, _) => {
            info!("Worker: Process {pid} ({id}) was terminated by signal {signal}");
            128 + (signal as i32)
        }
        _ => {
            warn!("Worker: Process {pid} ({id}) ended with unexpected status: {status:?}");
            255
        }
    };

    if event_tx
        .send(Event::ContainerStopped { id, exit_code })
        .await
        .is_err()
    {
        error!("Worker: Failed to send ContainerStopped event. Dispatcher may be down.");
    }
}
