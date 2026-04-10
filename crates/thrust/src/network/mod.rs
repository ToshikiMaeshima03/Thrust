//! ネットワーキング (Round 9)
//!
//! UDP ベースのクライアント・サーバー、Transform スナップショット同期、
//! クライアントサイド補間。最低限のリプリケーションサポート。
//!
//! 用途: マルチプレイヤーのプロトタイプ、LAN ゲーム
//!
//! ## 使い方
//! ```ignore
//! // サーバー側
//! let mut server = NetworkServer::bind("0.0.0.0:7777")?;
//! server.broadcast_snapshot(&snapshot)?;
//!
//! // クライアント側
//! let mut client = NetworkClient::connect("127.0.0.1:7777")?;
//! client.send_input(&input)?;
//! while let Some(snapshot) = client.poll_snapshot()? {
//!     // apply snapshot
//! }
//! ```

pub mod client;
pub mod protocol;
pub mod replication;
pub mod server;

pub use client::NetworkClient;
pub use protocol::{ClientInput, NetworkMessage, ServerSnapshot, SnapshotEntity};
pub use replication::{NetworkId, ReplicationMode, replicate_transforms};
pub use server::NetworkServer;
