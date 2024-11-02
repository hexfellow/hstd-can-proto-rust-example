use anyhow::anyhow;
use futures_util::{SinkExt, StreamExt};
use prost::Message as pMessage;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async_with_config, MaybeTlsStream, WebSocketStream};
use url::Url;
use crate::hstd::can_bridge_message::Msg;
use reqwest::Client;
use crate::hstd::{Info, Config};

pub mod hstd {
    include!(concat!(env!("OUT_DIR"), "/hstd.rs"));
}

pub const DEFAULT_BASE: &str = "10.233.233.1";

async fn get_info(base_url:&String) -> anyhow::Result<Info> {
    let client = Client::new();
    let url = Url::parse(base_url).unwrap().join("info").unwrap();
    let response = client.get(url).send().await.unwrap();
    let bin = response.bytes().await.unwrap();
    let info = hstd::Info::decode(&bin[..])?;
    Ok(info)
}

async fn get_config(base_url:&String) -> anyhow::Result<Config> {
    let client = Client::new();
    let url = Url::parse(base_url).unwrap().join("config").unwrap();
    let response = client.get(url).send().await.unwrap();
    println!("response {:?}", response);
    let bin = response.bytes().await.unwrap();
    let cfg = Config::decode(&bin[..])?;
    Ok(cfg)
}

async fn set_config(cfg: Config, base_url:&String) -> anyhow::Result<String> {
    let client = Client::new();
    let mut buffer:Vec<u8> = Vec::new();
    cfg.encode(&mut buffer).unwrap();
    let url = Url::parse(base_url).unwrap().join("config").unwrap();
    let response = client.post(url).body(buffer).send().await.unwrap();
    if response.status().is_success(){
        let t = response.text().await?;
        Ok(t)
    } else {
        let t = response.text().await?;
        Err(anyhow!(t))
    }
}


#[tokio::main]
async fn main() {
    // Make sure exit upon Ctrl-C
    ctrlc::set_handler(|| std::process::exit(0)).expect("Error setting Ctrl-C handler");
    let http_base_url = "http://".to_string() + DEFAULT_BASE;
    let info = get_info(&http_base_url).await.unwrap();
    println!("{:?}", info);
    let cfg = get_config(&http_base_url).await.unwrap();
    println!("{:?}", cfg);
    let cfg = Config{
        enabled: true,
        baud_rate: None,
        termination: None,
        name: None,
    };
    let response = set_config(cfg, &http_base_url).await.unwrap();
    println!("Set config response: {:?}", response);
    println!("Connecting to websocket");

    let ws_base_url = "ws://".to_string() + DEFAULT_BASE;
    let ws_stream = get_ws_stream(&ws_base_url).await;
    let (mut write, mut read) = ws_stream.split();

    // how to send messages
    tokio::spawn(async move {
        let mut write = write;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let msg = hstd::CanBridgeMessage {
                msg: Some(Msg::CanMsg(hstd::HexStdCanMsg {
                    id: 0x07FF01B0,
                    data: vec![0],
                    ext_id: true,
                    can_fd: false,
                    can_fd_brs: false,
                    receive_time: None,
                })),
            };
            write.send(Message::binary(msg.encode_to_vec())).await.unwrap();
        }
    });

    loop {
        tokio::select! {
            message = timeout(std::time::Duration::from_secs(3), read.next()) => {
            let message = match message {
                Ok(m) => {m}
                Err(_) => {
                    eprintln!("Did not receive anything from websocket for too long, are the cables still good? Quitting because the connection is very likely already dead.");
                    std::process::exit(255);
                }
            };
            match process_ws_msg(message){
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error handling websocket message: {:?}", e);
                }
            }
            }
        }
    }
}

fn process_ws_msg(message: Option<Result<Message, Error>>) -> anyhow::Result<()> {
    match message {
        Some(Ok(msg)) => {
            match msg {
                Message::Binary(msg) => {
                    let msg = hstd::CanBridgeMessage::decode(msg.as_slice())?;
                    let msg = msg.msg.ok_or(anyhow!("Empty message"))?;
                    match msg {
                        Msg::CanMsg(msg) => {
                            println!("Got CAN message: {:?}", msg);
                            Ok(())
                        }
                        Msg::LossIndication(loss) => {
                            eprintln!("Got LossIndication: {:?}", loss);
                            Err(anyhow!("loss"))
                        }
                    }
                }
                Message::Text(text) => {
                    eprintln!("OMG! Got Text type, Peer said {}", text);
                    Err(anyhow!(text))
                }
                _ => Ok(()),
            }
        }
        Some(Err(e)) => {
            eprintln!("Error receiving message from websocket: {:?}", e);
            Err(anyhow!(e))
        }
        None => {
            eprintln!("Received NULL message from websocket");
            Err(anyhow!("Got none"))
        }
    }
}

async fn get_ws_stream(base_url:&String) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let url = Url::parse(base_url).unwrap().join("hex-bridge").unwrap();
    let (ws_stream, _) = loop {
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            connect_async_with_config(url.clone(), None, true),
        )
        .await
        {
            Ok(ret) => match ret {
                Ok(ret) => break ret,
                Err(_) => {}
            },
            Err(_) => {}
        }
        eprintln!(
            "Error connecting to {:?}. Is the can-hub/router up?",
            url.as_str()
        );
    };
    ws_stream
}
