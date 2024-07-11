#![allow(dead_code)]

use std::string::String;

use anyhow::anyhow;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use prost::Message as pMessage;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async_with_config, MaybeTlsStream, WebSocketStream};
use url::Url;

use crate::bridge_msg_factory::{
    create_msg_get_interface_info, create_msg_request_config, create_msg_request_info,
    create_msg_request_protocol_version, create_msg_set_interface_to_default_config,
};
use crate::hstd::bridge::Msg;
use crate::hstd::bridge::Msg::CanMsg;
use crate::hstd::property::Prop;
use crate::hstd::DeviceClass::{EspCanHub, Pc, RouterCanHub};
use crate::hstd::HexBridgeProtocolVersion::HexBridgeProtocolV1;
use crate::hstd::MsgSourceType::{
    SourceCanHub, SourceExternalCan, SourceInternalCan, SourcePcRos, SourceXview,
};
use crate::hstd::{Bridge, DeviceClass};

pub mod hstd {
    include!(concat!(env!("OUT_DIR"), "/hstd.rs"));
}
mod bridge_msg_factory;
pub const DEFAULT_ADDR: &str = "ws://10.233.233.1/hex-bridge";

#[tokio::main]
async fn main() {
    // Make sure exit upon Ctrl-C
    // Note to work with rosrust, use rosrust::try_init_with_options or just do not set the handler
    ctrlc::set_handler(|| std::process::exit(0)).expect("Error setting Ctrl-C handler");
    let ws_stream = get_ws_stream().await;

    println!("Connected to CAN HUB. Getting Info about the CAN HUB...");
    let (mut write, mut read) = ws_stream.split();
    send_init_msg(&mut write).await;

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
            match handle_ws_msg(message).await{
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error handling websocket message: {:?}", e);
                }
            }
            }
        }
    }
}

async fn send_init_msg(write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) {
    /// Config the hub/router, ask for device name and disable all filters.
    // Request version
    let mut buf = Vec::new();
    create_msg_request_protocol_version()
        .encode(&mut buf)
        .unwrap();
    write.send(Message::binary(buf)).await.unwrap();
    // Actually we should wait here for a while to get the response, but we will just continue for sake of simplicity

    // Request device info
    let mut buf = Vec::new();
    create_msg_request_info().encode(&mut buf).unwrap();
    write.send(Message::binary(buf)).await.unwrap();

    // Request config
    let mut buf = Vec::new();
    create_msg_request_config().encode(&mut buf).unwrap();
    write.send(Message::binary(buf)).await.unwrap();

    // Get ext-can interface info
    let mut buf = Vec::new();
    create_msg_get_interface_info(SourceExternalCan)
        .encode(&mut buf)
        .unwrap();
    write.send(Message::binary(buf)).await.unwrap();

    // Reset ext-can interface to default config(500K baud rate, no filter, do not change term, etc.)
    let mut buf = Vec::new();
    create_msg_set_interface_to_default_config(SourceExternalCan)
        .encode(&mut buf)
        .unwrap();
    write.send(Message::binary(buf)).await.unwrap();
}

async fn handle_ws_msg(message: Option<Result<Message, Error>>) -> Result<(), anyhow::Error> {
    match message {
        Some(Ok(msg)) => {
            match msg {
                Message::Binary(msg) => {
                    let msg = match Bridge::decode(msg.as_slice()) {
                        Ok(msg) => msg,
                        Err(e) => {
                            eprintln!("Error decoding message: {:?}", e);
                            return Err(anyhow!(e));
                        }
                    };
                    // First check the protocol version, if mismatched, disconnect the websocket connection and exit
                    if msg.protocol_version != HexBridgeProtocolV1 as i32 {
                        // Exit now
                        eprintln!("Invalid protocol_version:{} sent by peer instead of {}. Please upgrade both the ros code and the can hub to latest software.", msg.protocol_version, HexBridgeProtocolV1 as i32);
                        std::process::exit(255);
                    }
                    match msg.msg {
                        None => {
                            eprintln!("Received empty message from peer");
                            Ok(())
                        }
                        Some(msg) => {
                            match msg {
                                Msg::PropertyResponse(p) => {
                                    match p.prop {
                                        Some(Prop::Config(c)) => {
                                            println!(
                                                "Connected Device name: {}, CAN ID {:#X}",
                                                c.name.unwrap_or("None".to_string()),
                                                c.can_full_id.unwrap_or(0)
                                            );
                                            Ok(())
                                        }
                                        Some(Prop::Info(info)) => {
                                            println!(
                                                "Connected Device info: Type {}, Firmware Version: {}",
                                                get_device_type_name(info.device_class), info.version.unwrap_or("Unknown".to_string())
                                            );
                                            Ok(())
                                        }
                                        Some(Prop::MsgSourceInfo(info)) => {
                                            if info.available == false {
                                                eprintln!(
                                                    "Interface {} is not available",
                                                    get_interface_name(info.channel)
                                                );
                                                Ok(())
                                            } else if info.can_ext_id_capable == false {
                                                eprintln!("Interface {} is not capable of extended CAN ID", get_interface_name(info.channel));
                                                Ok(())
                                            } else {
                                                println!("Interface {} info: Available: {}, EXT CAN: {}, CAN FD: {}", get_interface_name(info.channel), if info.available {"Yes"} else {"No"},
                                                         if info.can_ext_id_capable {"Yes"} else {"No"}, if info.can_fd_capable {"Yes"} else {"No"},);
                                                Ok(())
                                            }
                                        }
                                        Some(Prop::MsgSourceConfig(c)) => {
                                            println!("Configured interface {} to baud rate {} with filter id {:#X} and mask {:#X}, term: {}",
                                                     get_interface_name(c.channel), c.baud_rate, c.filter_id, c.filter_mask,
                                                     match c.term{
                                                         Some(res) => {if res {"On"} else {"Off"}}
                                                         None => "Unknown"
                                                     }
                                            );
                                            Ok(())
                                        }
                                        _ => Ok(()),
                                    }
                                }
                                CanMsg(msg) => {
                                    println!("Received CAN message: {:?}", msg);
                                    Ok(())
                                }
                                _ => Ok(()),
                            }
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

fn get_device_type_name(d: i32) -> String {
    if d == DeviceClass::OtherDevice as i32 {
        "Other Device".to_string()
    } else if d == EspCanHub as i32 {
        "Wireless CAN HUB".to_string()
    } else if d == RouterCanHub as i32 {
        "HEX-Router".to_string()
    } else if d == Pc as i32 {
        "PC".to_string()
    } else {
        "Unknown Device".to_string()
    }
}

fn get_interface_name(d: i32) -> String {
    if d == SourceExternalCan as i32 {
        "External CAN".to_string()
    } else if d == SourceCanHub as i32 {
        "CAN HUB It Self".to_string()
    } else if d == SourceInternalCan as i32 {
        "Internal CAN".to_string()
    } else if d == SourcePcRos as i32 {
        "ROS".to_string()
    } else if d == SourceXview as i32 {
        "XView".to_string()
    } else {
        "Unknown Interface".to_string()
    }
}

async fn get_ws_stream() -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let url = Url::parse(DEFAULT_ADDR).unwrap();
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
