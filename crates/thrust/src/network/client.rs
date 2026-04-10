//! ネットワーククライアント (Round 9)
//!
//! UDP ベースのシンプルなクライアント。サーバーに Hello → Welcome のハンドシェイク
//! 後、入力を送信し、スナップショットを受信する。

use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

use crate::error::{ThrustError, ThrustResult};
use crate::network::protocol::{ClientInput, NetworkMessage, ServerSnapshot};

pub struct NetworkClient {
    pub socket: UdpSocket,
    pub server_addr: SocketAddr,
    pub client_id: Option<u64>,
    pub current_tick: u64,
    /// 最新スナップショット
    pub latest_snapshot: Option<ServerSnapshot>,
    recv_buf: Vec<u8>,
}

impl NetworkClient {
    /// サーバーに接続する (Hello を即時送信、Welcome は次回 poll で受信)
    pub fn connect(server_addr: &str, client_name: &str) -> ThrustResult<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| ThrustError::Io {
            path: "0.0.0.0:0".into(),
            source: e,
        })?;
        socket
            .set_nonblocking(true)
            .map_err(|e| ThrustError::Physics(format!("set_nonblocking failed: {e}")))?;
        let addr: SocketAddr = server_addr.parse().map_err(|e: std::net::AddrParseError| {
            ThrustError::Physics(format!("address parse failed: {e}"))
        })?;
        // Hello 送信
        let hello = NetworkMessage::Hello {
            client_name: client_name.to_string(),
        };
        let _ = socket.send_to(&hello.to_bytes(), addr);
        Ok(Self {
            socket,
            server_addr: addr,
            client_id: None,
            current_tick: 0,
            latest_snapshot: None,
            recv_buf: vec![0u8; 65536],
        })
    }

    /// メッセージをポーリング
    pub fn poll_messages(&mut self) {
        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((n, _addr)) => {
                    if let Some(msg) = NetworkMessage::from_bytes(&self.recv_buf[..n]) {
                        self.handle_message(msg);
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    fn handle_message(&mut self, msg: NetworkMessage) {
        match msg {
            NetworkMessage::Welcome { client_id } => {
                self.client_id = Some(client_id);
            }
            NetworkMessage::Snapshot(snap) => {
                self.latest_snapshot = Some(snap);
            }
            _ => {}
        }
    }

    /// 入力を送信
    pub fn send_input(&self, input: &ClientInput) -> ThrustResult<()> {
        let msg = NetworkMessage::Input(input.clone());
        self.socket
            .send_to(&msg.to_bytes(), self.server_addr)
            .map_err(|e| ThrustError::Io {
                path: format!("{}", self.server_addr).into(),
                source: e,
            })?;
        Ok(())
    }

    /// 現在の最新スナップショットを取得 (clone)
    pub fn snapshot(&self) -> Option<ServerSnapshot> {
        self.latest_snapshot.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.client_id.is_some()
    }

    pub fn tick(&mut self) {
        self.current_tick += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_to_unreachable() {
        // 存在しないサーバーでも UDP は失敗しない
        let result = NetworkClient::connect("127.0.0.1:1", "Test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_initial_state() {
        let client = NetworkClient::connect("127.0.0.1:1", "Test").unwrap();
        assert!(!client.is_connected());
        assert_eq!(client.current_tick, 0);
        assert!(client.snapshot().is_none());
    }

    #[test]
    fn test_invalid_address() {
        let result = NetworkClient::connect("invalid_address", "Test");
        assert!(result.is_err());
    }

    #[test]
    fn test_tick_increments() {
        let mut client = NetworkClient::connect("127.0.0.1:1", "Test").unwrap();
        client.tick();
        client.tick();
        assert_eq!(client.current_tick, 2);
    }
}
