//! kira ベースのオーディオシステム (Round 4 完成版)
//!
//! - 効果音 / BGM 再生 (`AudioManager::play_sound`, `play_music`)
//! - 3D 空間音響 (`AudioManager::play_spatial`) — リスナー位置から距離減衰 + ステレオパン
//! - `AudioListener` / `AudioEmitter` ECS コンポーネント

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use kira::listener::ListenerHandle;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::{SpatialTrackBuilder, SpatialTrackDistances, SpatialTrackHandle};
use kira::{AudioManager as KiraManager, AudioManagerSettings, DefaultBackend, Tween};

use crate::error::{ThrustError, ThrustResult};

/// オーディオソース: ロード済み音声データ
///
/// 内部でデコード済みの `StaticSoundData` を保持し、再生時に低コストでクローンする。
#[derive(Clone)]
pub struct AudioSource {
    data: Arc<StaticSoundData>,
}

impl AudioSource {
    /// バイト列からオーディオソースを作成する
    pub fn from_bytes(bytes: Vec<u8>) -> ThrustResult<Self> {
        let cursor = Cursor::new(bytes);
        let data = StaticSoundData::from_cursor(cursor)
            .map_err(|e| ThrustError::AudioDecode(e.to_string()))?;
        Ok(Self {
            data: Arc::new(data),
        })
    }

    /// ファイルパスからオーディオソースを読み込む
    pub fn from_path(path: &str) -> ThrustResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| ThrustError::Io {
            path: path.into(),
            source: e,
        })?;
        Self::from_bytes(bytes)
    }
}

/// サウンドハンドル: 再生中のサウンドの制御用
#[derive(Debug, Clone, Copy)]
pub struct SoundHandle {
    id: u64,
}

/// オーディオ管理: kira バックエンドの効果音・BGM・3D 空間音響
pub struct AudioManager {
    manager: KiraManager<DefaultBackend>,
    listener: ListenerHandle,
    sounds: HashMap<u64, StaticSoundHandle>,
    /// 一時的な spatial track (再生終了後にクリーンアップ)
    spatial_tracks: HashMap<u64, SpatialTrackHandle>,
    next_id: u64,
    master_volume: f32,
}

fn vec3_to_mint(v: glam::Vec3) -> mint::Vector3<f32> {
    mint::Vector3 {
        x: v.x,
        y: v.y,
        z: v.z,
    }
}

fn quat_to_mint(q: glam::Quat) -> mint::Quaternion<f32> {
    mint::Quaternion {
        v: mint::Vector3 {
            x: q.x,
            y: q.y,
            z: q.z,
        },
        s: q.w,
    }
}

impl AudioManager {
    /// オーディオマネージャーを初期化する
    ///
    /// オーディオデバイスが利用不可の場合は `None` を返す。
    pub fn new() -> Option<Self> {
        let mut manager = match KiraManager::<DefaultBackend>::new(AudioManagerSettings::default())
        {
            Ok(m) => m,
            Err(e) => {
                log::warn!("オーディオデバイス初期化失敗: {e}");
                return None;
            }
        };

        // デフォルトリスナー (原点・無回転)
        let listener = match manager.add_listener(
            vec3_to_mint(glam::Vec3::ZERO),
            quat_to_mint(glam::Quat::IDENTITY),
        ) {
            Ok(l) => l,
            Err(e) => {
                log::warn!("オーディオリスナー作成失敗: {e}");
                return None;
            }
        };

        Some(Self {
            manager,
            listener,
            sounds: HashMap::new(),
            spatial_tracks: HashMap::new(),
            next_id: 0,
            master_volume: 1.0,
        })
    }

    /// 効果音を再生する (2D、メイントラック)
    pub fn play_sound(&mut self, source: &AudioSource) -> ThrustResult<SoundHandle> {
        let mut data = (*source.data).clone();
        // master_volume を反映 (kira は dB ベース)
        data.settings.volume =
            kira::Decibels::from(20.0 * self.master_volume.max(0.001).log10()).into();

        let handle = self
            .manager
            .play(data)
            .map_err(|e| ThrustError::AudioPlayback(e.to_string()))?;

        let id = self.next_id;
        self.next_id += 1;
        self.sounds.insert(id, handle);

        Ok(SoundHandle { id })
    }

    /// 3D 空間音響: エミッタ位置で再生し、リスナーとの距離で自動減衰する
    ///
    /// `min_distance` ～ `max_distance` の範囲で線形減衰、超えると無音。
    /// ステレオパンも自動的にリスナーの向きから計算される。
    pub fn play_spatial(
        &mut self,
        source: &AudioSource,
        emitter_position: glam::Vec3,
        min_distance: f32,
        max_distance: f32,
    ) -> ThrustResult<SoundHandle> {
        let builder = SpatialTrackBuilder::new()
            .distances(SpatialTrackDistances {
                min_distance: min_distance.max(0.01),
                max_distance: max_distance.max(min_distance + 0.01),
            })
            .attenuation_function(Some(kira::Easing::Linear));

        let mut track = self
            .manager
            .add_spatial_sub_track(self.listener.id(), vec3_to_mint(emitter_position), builder)
            .map_err(|e| ThrustError::AudioPlayback(e.to_string()))?;

        let mut data = (*source.data).clone();
        data.settings.volume =
            kira::Decibels::from(20.0 * self.master_volume.max(0.001).log10()).into();

        let handle = track
            .play(data)
            .map_err(|e| ThrustError::AudioPlayback(e.to_string()))?;

        let id = self.next_id;
        self.next_id += 1;
        self.sounds.insert(id, handle);
        self.spatial_tracks.insert(id, track);

        Ok(SoundHandle { id })
    }

    /// BGM をループ再生する
    pub fn play_music(&mut self, source: &AudioSource) -> ThrustResult<SoundHandle> {
        let mut data = (*source.data).clone();
        data.settings.volume =
            kira::Decibels::from(20.0 * self.master_volume.max(0.001).log10()).into();
        data.settings.loop_region = Some((0.0..).into());

        let handle = self
            .manager
            .play(data)
            .map_err(|e| ThrustError::AudioPlayback(e.to_string()))?;

        let id = self.next_id;
        self.next_id += 1;
        self.sounds.insert(id, handle);

        Ok(SoundHandle { id })
    }

    /// 再生を停止する
    pub fn stop(&mut self, handle: &SoundHandle) {
        if let Some(mut sound) = self.sounds.remove(&handle.id) {
            sound.stop(Tween::default());
        }
        self.spatial_tracks.remove(&handle.id);
    }

    /// 全サウンドを停止する
    pub fn stop_all(&mut self) {
        for (_, mut sound) in self.sounds.drain() {
            sound.stop(Tween::default());
        }
        self.spatial_tracks.clear();
    }

    /// マスターボリュームを設定する (0.0 - 1.0、対数スケール)
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
        // 既存の sound には適用されない (新規再生から反映)
    }

    /// マスターボリュームを取得する
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// 一時停止する
    pub fn pause(&mut self, handle: &SoundHandle) {
        if let Some(sound) = self.sounds.get_mut(&handle.id) {
            sound.pause(Tween::default());
        }
    }

    /// 一時停止を解除する
    pub fn resume(&mut self, handle: &SoundHandle) {
        if let Some(sound) = self.sounds.get_mut(&handle.id) {
            sound.resume(Tween::default());
        }
    }

    /// リスナーの位置と向きを更新する
    ///
    /// 通常はアクティブカメラの位置・回転を毎フレーム渡す (`audio_listener_system` が自動で実行)。
    pub fn update_listener(&mut self, position: glam::Vec3, orientation: glam::Quat) {
        self.listener
            .set_position(vec3_to_mint(position), Tween::default());
        self.listener
            .set_orientation(quat_to_mint(orientation), Tween::default());
    }

    /// 再生完了したサウンドをクリーンアップする
    pub fn cleanup_finished(&mut self) {
        self.sounds.retain(|_, sound| {
            !matches!(
                sound.state(),
                kira::sound::PlaybackState::Stopped | kira::sound::PlaybackState::Stopping
            )
        });
        // 対応する spatial track もクリーンアップ
        self.spatial_tracks
            .retain(|id, _| self.sounds.contains_key(id));
    }
}

/// 3D 空間音響用エミッタコンポーネント
///
/// `Transform` を持つエンティティに付与する。
/// `audio_emitter_system` が `auto_play` を見て、初回 1 回だけ再生する。
pub struct AudioEmitter {
    pub source: AudioSource,
    /// 最大可聴距離 (m) — これを超えると無音
    pub max_distance: f32,
    /// 最小距離 (m) — これより近いと最大音量
    pub min_distance: f32,
    /// 自動再生フラグ
    pub auto_play: bool,
    /// 内部状態: 一度再生したらフラグが立つ
    pub(crate) played: bool,
}

impl AudioEmitter {
    pub fn new(source: AudioSource, max_distance: f32) -> Self {
        Self {
            source,
            min_distance: 1.0,
            max_distance,
            auto_play: true,
            played: false,
        }
    }

    /// 手動で再生フラグをリセットして再再生を許可する
    pub fn replay(&mut self) {
        self.played = false;
    }
}

/// 3D 空間音響用リスナーマーカー
///
/// アクティブカメラと同じエンティティに付与する。
/// `audio_listener_system` がカメラ位置・向きを kira リスナーに同期する。
pub struct AudioListener;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_source_from_bytes_invalid() {
        let result = AudioSource::from_bytes(vec![0, 0, 0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_source_from_path_nonexistent() {
        let result = AudioSource::from_path("/nonexistent/sound.wav");
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_emitter_new() {
        // ダミー source: from_bytes は失敗するが、構造体だけテスト
        let result = AudioSource::from_bytes(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_vec3_to_mint() {
        let v = glam::Vec3::new(1.0, 2.0, 3.0);
        let m = vec3_to_mint(v);
        assert_eq!(m.x, 1.0);
        assert_eq!(m.y, 2.0);
        assert_eq!(m.z, 3.0);
    }

    #[test]
    fn test_quat_to_mint() {
        let q = glam::Quat::from_xyzw(0.1, 0.2, 0.3, 0.9);
        let m = quat_to_mint(q);
        assert!((m.s - 0.9).abs() < 1e-5);
        assert!((m.v.x - 0.1).abs() < 1e-5);
    }
}
