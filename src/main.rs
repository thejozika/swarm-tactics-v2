use std::error::Error;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use axum::{Extension, Form, Json, Router};
use axum::extract::{State, WebSocketUpgrade};
use axum::extract::ws::{WebSocket,Message as WebSocketMessage};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncWriteExt, AsyncReadExt};
use serde::{Serialize, Deserialize};
use rmp_serde::{Serializer};

#[derive(Serialize, Deserialize, Debug)]
struct MyData {
    id: u32,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct BaseData {
    sensor_1: [[i32;4];3],
    sensor_2: [[i32;4];3],
}

#[derive(Serialize, Deserialize, Debug)]
struct UnitData {
    id: u32,
    sensor_1: [[i32;3];3],
    sensor_2: [[i32;3];3]
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    base: BaseData,
    units: Vec<UnitData>
}

#[derive(Serialize, Deserialize, Debug)]
struct Answer {
    base:BaseActions,
    units:Vec<UnitAction>
}

#[derive(Serialize, Deserialize, Debug)]
struct UnitAction {
    id:u32,
    action:UnitActionOption
}

#[derive(Serialize, Deserialize, Debug)]
enum UnitActionOption {
    MOVE,
    TURN_LEFT,
    TURN_RIGHT,
    ATTACK,
    NOP
}

#[derive(Serialize, Deserialize, Debug)]
enum BaseActions {
    SPAWN,
    NOP
}

#[derive(Deserialize)]
struct FormData {
    ip: String,
    port: String,
    refresh_rate: u64,
}

#[derive(Clone)]
struct AppState {
    ip: Arc<Mutex<String>>,
    port: Arc<Mutex<u32>>,
    refresh_rate: Arc<Mutex<u64>>,
    tx: mpsc::Sender<String>,
    rx: Arc<Mutex<mpsc::Receiver<String>>>
}

async fn form_page() -> Html<&'static str> {
    Html(include_str!("form.html"))  // Ensure the HTML file is in your src directory
}


async fn handle_form(State(state): State<AppState>, Json(data): Json<FormData>) -> Json<String> {
    let mut ip = state.ip.lock().await;
    *ip = data.ip;
    let mut port = state.port.lock().await;
    *port = data.port.parse::<u32>().unwrap();
    let mut refresh_rate = state.refresh_rate.lock().await;
    *refresh_rate = data.refresh_rate;
    tokio::spawn(start_tcp_connection(state.clone()));
    Json(format!("Data submitted! IP: {}, Port: {}, Refresh rate: {}", *ip, *port, *refresh_rate))
}

async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.rx.lock().await;
    while let Some(message) = rx.recv().await {
        if socket.send(WebSocketMessage::Text(message)).await.is_err() {
            break; // Connection was closed
        }
    }

}

async fn send_message(stream: &mut TcpStream, message: &Message) -> io::Result<()> {
    let mut buf = Vec::new();
    message.serialize(&mut Serializer::new(&mut buf)).unwrap();

    let message_size = buf.len() as u32; // Ensure this is in network byte order if cross-platform
    let message_size_bytes = message_size.to_be_bytes(); // Converts u32 to bytes in big-endian format

    stream.write_all(&message_size_bytes).await?;
    stream.write_all(&buf).await?;
    stream.flush().await?;
    Ok(())
}

async fn receive_response(stream: &mut TcpStream) -> io::Result<Answer> {
    let mut length_buf = [0u8; 4];
    stream.read_exact(&mut length_buf).await?;
    let length = u32::from_be_bytes(length_buf) as usize;
    let mut response_buf = vec![0u8; length];
    stream.read_exact(&mut response_buf).await?;

    let response: Answer = rmp_serde::from_slice(&response_buf).unwrap();
    Ok(response)
}

fn create_message() -> Message {
    return Message {
        base: BaseData {
            sensor_1: [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]],
            sensor_2: [[13, 14, 15, 16], [17, 18, 19, 20], [21, 22, 23, 24]],
        },
        units: vec![
            UnitData {
                id: 10,
                sensor_1: [[10, 20, 30], [40, 50, 60], [70, 80, 90]],
                sensor_2: [[100, 110, 120], [130, 140, 150], [160, 170, 180]],
            },
        ],
    };
}

async fn start_tcp_connection(state: AppState) {
    let ip = state.ip.lock().await.clone();
    let port = *state.port.lock().await;
    let refresh_rate = *state.refresh_rate.lock().await;

    let addr = format!("{}:{}", ip, port);
    if let Ok(mut stream) = TcpStream::connect(addr).await {
        loop {
            let message = create_message();  // Function to create a Message
            if let Ok(_) = send_message(&mut stream, &message).await {
                println!("Data sent...");
                if let Ok(response) = receive_response(&mut stream).await {
                    //let mut last_response = state.last_response.lock().await;
                    println!("Received response: {:?}", response);
                    //*last_response = Some(response);
                    let response_json = serde_json::to_string(&response).unwrap();
                    if state.tx.send(response_json).await.is_err() {
                        eprintln!("Failed to send response through channel");
                        break;
                    }

                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(refresh_rate as u64)).await;
        }
    } else {
        eprintln!("Failed to connect to {}:{}", ip, port);
    }
}


#[tokio::main]
async fn main() -> Result<(),Box<dyn Error>> {
    let (tx, rx) = mpsc::channel(32);  // 32 is the channel capacity


    let shared_state = AppState {
        ip: Arc::new(Mutex::new(String::new())),
        port: Arc::new(Mutex::new(0)),
        refresh_rate: Arc::new(Mutex::new(0)),
        tx,
        rx: Arc::new(Mutex::new(rx)),
    };

    let routes_all = Router::new()
        .route("/", get(form_page))
        .route("/submit", post(handle_form))
        .route("/ws", get(websocket_handler))
        .with_state(shared_state);

    let listener = TcpListener::bind("127.0.0.1:8888").await?;
    println!("LISTENING on {:?}\n", listener.local_addr());
    axum::serve(listener, routes_all.into_make_service()).await?;
    Ok(())
}