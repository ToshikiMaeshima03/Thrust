use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

/// オーディオソース: ロード済み音声データ
///
/// `Arc` でラップされ、`AssetManager` 経由でキャッシュ共有される。
#[derive(Clone)]
pub struct AudioSource {
    data: Arc<Vec<u8>>,
}

impl AudioSource {
    /// バイト列からオーディオソースを作成する
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            data: Arc::new(bytes),
        }
    }

    /// ファイルパスからオーディオソースを読み込む
    pub fn from_path(path: &str) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("音声ファイル読み込みエラー: {e}"))?;
        Ok(Self::from_bytes(bytes))
    }

    /// デコーダーを生成する（再生のたびに呼び出す）
    fn decoder(&self) -> Result<Decoder<Cursor<Vec<u8>>>, String> {
        let cursor = Cursor::new((*self.data).clone());
        Decoder::new(cursor).map_err(|e| format!("音声デコードエラー: {e}"))
    }
}

/// サウンドハンドル: 再生中のサウンドの制御用
pub struct SoundHandle {
    id: u64,
}

/// オーディオ管理: 効果音・BGM の再生制御
///
/// rodio の `OutputStream` + `Sink` をラップする。
/// 複数同時再生をサポート。
pub struct AudioManager {
    /// rodio のオーディオストリーム（Drop で停止するため保持が必要）
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,

    /// 再生中のサウンド
    sinks: HashMap<u64, Sink>,
    next_id: u64,

    /// マスターボリューム (0.0 - 1.0)
    master_volume: f32,
}

impl AudioManager {
    /// オーディオマネージャーを初期化する
    ///
    /// オーディオデバイスが利用不可の場合は `None` を返す。
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = match OutputStream::try_default() {
            Ok(pair) => pair,
            Err(e) => {
                log::warn!("オーディオデバイス初期化失敗: {e}");
                return None;
            }
        };

        Some(Self {
            _stream: stream,
            stream_handle,
            sinks: HashMap::new(),
            next_id: 0,
            master_volume: 1.0,
        })
    }

    /// 効果音を再生する
    pub fn play_sound(&mut self, source: &AudioSource) -> Result<SoundHandle, String> {
        let decoder = source.decoder()?;
        let sink =
            Sink::try_new(&self.stream_handle).map_err(|e| format!("Sink 作成エラー: {e}"))?;
        sink.set_volume(self.master_volume);
        sink.append(decoder);

        let id = self.next_id;
        self.next_id += 1;
        self.sinks.insert(id, sink);

        Ok(SoundHandle { id })
    }

    /// BGM をループ再生する
    pub fn play_music(&mut self, source: &AudioSource) -> Result<SoundHandle, String> {
        let decoder = source.decoder()?;
        let sink =
            Sink::try_new(&self.stream_handle).map_err(|e| format!("Sink 作成エラー: {e}"))?;
        sink.set_volume(self.master_volume);
        sink.append(decoder.repeat_infinite());

        let id = self.next_id;
        self.next_id += 1;
        self.sinks.insert(id, sink);

        Ok(SoundHandle { id })
    }

    /// 再生を停止する
    pub fn stop(&mut self, handle: &SoundHandle) {
        if let Some(sink) = self.sinks.remove(&handle.id) {
            sink.stop();
        }
    }

    /// 全サウンドを停止する
    pub fn stop_all(&mut self) {
        for (_, sink) in self.sinks.drain() {
            sink.stop();
        }
    }

    /// 特定サウンドの音量を設定する (0.0 - 1.0)
    pub fn set_volume(&mut self, handle: &SoundHandle, volume: f32) {
        if let Some(sink) = self.sinks.get(&handle.id) {
            sink.set_volume(volume * self.master_volume);
        }
    }

    /// マスターボリュームを設定する (0.0 - 1.0)
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
        for sink in self.sinks.values() {
            sink.set_volume(self.master_volume);
        }
    }

    /// マスターボリュームを取得する
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// 再生が一時停止中かどうかを確認する
    pub fn is_paused(&self, handle: &SoundHandle) -> bool {
        self.sinks
            .get(&handle.id)
            .is_some_and(|sink| sink.is_paused())
    }

    /// 一時停止する
    pub fn pause(&self, handle: &SoundHandle) {
        if let Some(sink) = self.sinks.get(&handle.id) {
            sink.pause();
        }
    }

    /// 一時停止を解除する
    pub fn resume(&self, handle: &SoundHandle) {
        if let Some(sink) = self.sinks.get(&handle.id) {
            sink.play();
        }
    }

    /// 再生完了した Sink をクリーンアップする
    pub fn cleanup_finished(&mut self) {
        self.sinks.retain(|_, sink| !sink.empty());
    }
}
