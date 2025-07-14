use image_service::IMAGE_SERVICE_SOCKET;
use log::{info, warn};
use proto_definitions::image_service::image_service_client::ImageServiceClient;
use proto_definitions::vm_service::{
    stream_vm_console_request as console_input, AttachConsoleMessage, ConsoleData,
    StreamVmConsoleRequest, StreamVmConsoleResponse,
};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{
    transport::{Channel, Endpoint, Error as TonicTransportError, Uri},
    Status, Streaming,
};
use tower::service_fn;

pub(crate) async fn get_image_service_client(
) -> Result<ImageServiceClient<Channel>, TonicTransportError> {
    let socket_path = PathBuf::from(IMAGE_SERVICE_SOCKET);
    Endpoint::try_from("http://[::1]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            tokio::net::UnixStream::connect(socket_path.clone())
        }))
        .await
        .map(ImageServiceClient::new)
}

pub(crate) async fn get_attach_message(
    stream: &mut Streaming<StreamVmConsoleRequest>,
) -> Result<String, Status> {
    match stream.next().await {
        Some(Ok(msg)) => match msg.payload {
            Some(console_input::Payload::Attach(AttachConsoleMessage { vm_id })) => Ok(vm_id),
            _ => Err(Status::invalid_argument(
                "First message must be an Attach message.",
            )),
        },
        Some(Err(e)) => Err(e),
        None => Err(Status::invalid_argument(
            "Client disconnected before sending Attach message.",
        )),
    }
}

pub(crate) async fn bridge_console_streams(
    socket_path: PathBuf,
    mut grpc_input: Streaming<StreamVmConsoleRequest>,
    grpc_output: mpsc::Sender<Result<StreamVmConsoleResponse, Status>>,
) {
    let vm_id = socket_path
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap_or("unknown")
        .to_string();

    let socket = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            let err_msg = format!("Failed to connect to console socket at {socket_path:?}: {e}");
            let _ = grpc_output.send(Err(Status::unavailable(err_msg))).await;
            return;
        }
    };

    let (mut socket_reader, mut socket_writer) = tokio::io::split(socket);
    let grpc_output_clone = grpc_output.clone();
    let read_task_vm_id = vm_id.clone();

    let read_task = tokio::spawn(async move {
        let mut buf = vec![0; 4096];
        loop {
            tokio::select! {
                biased;
                _ = grpc_output_clone.closed() => {
                    info!("VMM_HELPER (Console {}): gRPC client disconnected, terminating read task.", &read_task_vm_id);
                    break;
                }
                read_result = socket_reader.read(&mut buf) => {
                    match read_result {
                        Ok(0) => {
                            info!("VMM_HELPER (Console {}): Console socket closed (EOF).", &read_task_vm_id);
                            break;
                        }
                        Ok(n) => {
                            let output_msg = StreamVmConsoleResponse { output: buf[..n].to_vec() };
                            if grpc_output_clone.send(Ok(output_msg)).await.is_err() {
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("Error reading from console socket: {e}");
                            let _ = grpc_output_clone.send(Err(Status::internal(err_msg))).await;
                            break;
                        }
                    }
                }
            }
        }
    });

    let write_task_vm_id = vm_id.clone();
    let write_task = tokio::spawn(async move {
        while let Some(result) = grpc_input.next().await {
            match result {
                Ok(msg) => match msg.payload {
                    Some(console_input::Payload::Data(ConsoleData { input })) => {
                        if let Err(e) = socket_writer.write_all(&input).await {
                            warn!("VMM_HELPER (Console {}): Failed to write to socket: {}. VM may have shut down.", &write_task_vm_id, e);
                            break;
                        }
                    }
                    Some(console_input::Payload::Attach(_)) => {
                        let _ = grpc_output
                            .send(Err(Status::invalid_argument(
                                "Cannot send Attach message more than once.",
                            )))
                            .await;
                        break;
                    }
                    None => {
                        let _ = grpc_output
                            .send(Err(Status::invalid_argument("Empty ConsoleInput payload.")))
                            .await;
                        break;
                    }
                },
                Err(e) => {
                    warn!("VMM_HELPER (Console {}): Error reading from gRPC client stream: {}", &write_task_vm_id, e);
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = read_task => {},
        _ = write_task => {},
    }
}