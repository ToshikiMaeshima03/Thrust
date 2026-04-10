//! ネットワークプロトコル定義 (Round 9)
//!
//! クライアント↔サーバー間で交換するメッセージ。bincode 風の単純な手書き
//! シリアライズ + serde JSON 版を両方サポート。

use serde::{Deserialize, Serialize};

/// 1 エンティティのスナップショット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotEntity {
    pub network_id: u64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

/// サーバー → クライアントのスナップショット
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSnapshot {
    pub tick: u64,
    pub timestamp: f64,
    pub entities: Vec<SnapshotEntity>,
}

/// クライアント → サーバーの入力
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInput {
    pub tick: u64,
    pub move_dir: [f32; 3],
    pub look_dir: [f32; 3],
    pub buttons: u32,
}

/// プロトコルメッセージ (上位)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Hello { client_name: String },
    Welcome { client_id: u64 },
    Snapshot(ServerSnapshot),
    Input(ClientInput),
    Disconnect { reason: String },
}

impl NetworkMessage {
    /// JSON にシリアライズ
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// JSON からデシリアライズ
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_entity_roundtrip() {
        let e = SnapshotEntity {
            network_id: 42,
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: SnapshotEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.network_id, 42);
    }

    #[test]
    fn test_message_hello() {
        let msg = NetworkMessage::Hello {
            client_name: "Player1".to_string(),
        };
        let bytes = msg.to_bytes();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();
        match parsed {
            NetworkMessage::Hello { client_name } => assert_eq!(client_name, "Player1"),
            _ => panic!("不正なメッセージ"),
        }
    }

    #[test]
    fn test_message_snapshot() {
        let snap = ServerSnapshot {
            tick: 100,
            timestamp: 5.0,
            entities: vec![SnapshotEntity {
                network_id: 1,
                position: [0.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0; 3],
            }],
        };
        let msg = NetworkMessage::Snapshot(snap);
        let bytes = msg.to_bytes();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();
        match parsed {
            NetworkMessage::Snapshot(s) => {
                assert_eq!(s.tick, 100);
                assert_eq!(s.entities.len(), 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn test_invalid_bytes() {
        let bytes = b"not valid json";
        assert!(NetworkMessage::from_bytes(bytes).is_none());
    }

    #[test]
    fn test_input_serialization() {
        let input = ClientInput {
            tick: 50,
            move_dir: [1.0, 0.0, 0.5],
            look_dir: [0.0, 0.0, -1.0],
            buttons: 0b0011,
        };
        let bytes = NetworkMessage::Input(input).to_bytes();
        let parsed = NetworkMessage::from_bytes(&bytes).unwrap();
        match parsed {
            NetworkMessage::Input(i) => {
                assert_eq!(i.tick, 50);
                assert_eq!(i.buttons, 0b0011);
            }
            _ => panic!(),
        }
    }
}
