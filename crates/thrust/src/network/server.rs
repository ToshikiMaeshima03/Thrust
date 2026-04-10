//! ネットワークサーバー (Round 9)
//!
//! UDP ベースのシンプルなサーバー。クライアントからの Hello を受け取り、
//! スナップショットを broadcast する。
//!
//! ノンブロッキング IO で、ゲームループ内から `poll_messages()` を呼ぶ前提。

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

use crate::error::{ThrustError, ThrustResult};
use crate::network::protocol::{NetworkMessage, ServerSnapshot};

/// 接続中のクライアント情報
#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub id: u64,
    pub addr: SocketAddr,
    pub name: String,
    pub last_seen_tick: u64,
}

/// UDP サーバー
pub struct NetworkServer {
    pub socket: UdpSocket,
    pub clients: HashMap<SocketAddr, ConnectedClient>,
    pub next_client_id: u64,
    pub current_tick: u64,
    /// 受信バッファ (1 メッセージ最大 64 KB)
    recv_buf: Vec<u8>,
}

impl NetworkServer {
    pub fn bind(addr: &str) -> ThrustResult<Self> {
        let socket = UdpSocket::bind(addr).map_err(|e| ThrustError::Io {
            path: addr.into(),
            source: e,
        })?;
        socket
            .set_nonblocking(true)
            .map_err(|e| ThrustError::Physics(format!("set_nonblocking failed: {e}")))?;
        Ok(Self {
            socket,
            clients: HashMap::new(),
            next_client_id: 1,
            current_tick: 0,
            recv_buf: vec![0u8; 65536],
        })
    }

    /// 受信メッセージを処理する。返り値は (送信元アドレス, メッセージ) のリスト
    pub fn poll_messages(&mut self) -> Vec<(SocketAddr, NetworkMessage)> {
        let mut out = Vec::new();
        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((n, addr)) => {
                    if let Some(msg) = NetworkMessage::from_bytes(&self.recv_buf[..n]) {
                        // Hello を見つけたらクライアント登録
                        if let NetworkMessage::Hello { client_name } = &msg {
                            let id = self.next_client_id;
                            self.next_client_id += 1;
                            self.clients.insert(
                                addr,
                                ConnectedClient {
                                    id,
                                    addr,
                                    name: client_name.clone(),
                                    last_seen_tick: self.current_tick,
                                },
                            );
                            // Welcome を送り返す
                            let welcome = NetworkMessage::Welcome { client_id: id };
                            let _ = self.socket.send_to(&welcome.to_bytes(), addr);
                        } else if let Some(c) = self.clients.get_mut(&addr) {
                            c.last_seen_tick = self.current_tick;
                        }
                        out.push((addr, msg));
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        out
    }

    /// 全クライアントに snapshot を broadcast
    pub fn broadcast_snapshot(&self, snapshot: &ServerSnapshot) -> ThrustResult<()> {
        let msg = NetworkMessage::Snapshot(snapshot.clone());
        let bytes = msg.to_bytes();
        for addr in self.clients.keys() {
            let _ = self.socket.send_to(&bytes, addr);
        }
        Ok(())
    }

    /// tick をインクリメント
    pub fn tick(&mut self) {
        self.current_tick += 1;
    }

    /// 一定 tick 以上応答がないクライアントを切断
    pub fn cleanup_stale_clients(&mut self, timeout_ticks: u64) {
        let cur = self.current_tick;
        self.clients
            .retain(|_, c| cur.saturating_sub(c.last_seen_tick) < timeout_ticks);
    }

    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_to_ephemeral_port() {
        let server = NetworkServer::bind("127.0.0.1:0").unwrap();
        assert!(server.socket.local_addr().is_ok());
    }

    #[test]
    fn test_initial_state() {
        let server = NetworkServer::bind("127.0.0.1:0").unwrap();
        assert_eq!(server.clients.len(), 0);
        assert_eq!(server.next_client_id, 1);
        assert_eq!(server.current_tick, 0);
    }

    #[test]
    fn test_tick_increments() {
        let mut server = NetworkServer::bind("127.0.0.1:0").unwrap();
        server.tick();
        server.tick();
        assert_eq!(server.current_tick, 2);
    }

    #[test]
    fn test_poll_no_messages() {
        let mut server = NetworkServer::bind("127.0.0.1:0").unwrap();
        let msgs = server.poll_messages();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_cleanup_stale_clients_empty() {
        let mut server = NetworkServer::bind("127.0.0.1:0").unwrap();
        server.cleanup_stale_clients(60);
        assert_eq!(server.client_count(), 0);
    }
}
