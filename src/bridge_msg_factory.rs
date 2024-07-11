use crate::hstd::bridge::Msg::{CanMsg, PropertyRequest, ProtocolVersionRequest, SetProperty};
use crate::hstd::property::Prop;
use crate::hstd::HexBridgeProtocolVersion::{HexBridgeProtocolRequest, HexBridgeProtocolV1};
use crate::hstd::{
    Bridge, DeviceConfig, DeviceInfo, HexStdCanMsg, MessageSourceConfig, MessageSourceInfo,
    MsgSourceType, Property,
};

pub fn create_msg_get_interface_info(interface: MsgSourceType) -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolV1),
        msg: Option::from(PropertyRequest(Property {
            prop: Some(Prop::MsgSourceInfo(MessageSourceInfo {
                channel: interface as i32,
                available: false,
                can_ext_id_capable: false,
                can_fd_capable: false,
            })),
        })),
    }
}

pub fn create_msg_set_interface_to_default_config(interface: MsgSourceType) -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolV1),
        msg: Option::from(SetProperty(Property {
            prop: Some(Prop::MsgSourceConfig(MessageSourceConfig {
                channel: interface as i32,
                baud_rate: 500000,
                enabled: true,
                filter_id: 0,
                filter_mask: 0,
                term: None,
            })),
        })),
    }
}

pub fn create_msg_request_config() -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolV1),
        msg: Option::from(PropertyRequest(Property {
            prop: Some(Prop::Config(DeviceConfig {
                name: None,
                can_full_id: None,
            })),
        })),
    }
}

pub fn create_msg_request_protocol_version() -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolRequest),
        msg: Option::from(ProtocolVersionRequest(true)),
    }
}

pub fn create_msg_request_info() -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolV1),
        msg: Option::from(PropertyRequest(Property {
            prop: Some(Prop::Info(DeviceInfo {
                device_class: 0,
                version: None,
            })),
        })),
    }
}

pub fn create_msg_ext_id_can_msg(id: u32, data: Vec<u8>, source: i32) -> Bridge {
    Bridge {
        protocol_version: i32::from(HexBridgeProtocolV1),
        msg: Option::from(CanMsg(HexStdCanMsg {
            id,
            data,
            ext_id: true,
            can_fd: false,
            source,
            destination: None,
            receive_time: None,
        })),
    }
}
