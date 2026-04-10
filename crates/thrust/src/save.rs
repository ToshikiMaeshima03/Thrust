//! セーブ/ロードシステム (Round 8)
//!
//! ゲーム状態を JSON 形式で永続化するヘルパー。
//! `SaveData` は任意の `Serialize + Deserialize` 型を保持でき、
//! `save_to_file` / `load_from_file` でディスクにロード/セーブ可能。
//!
//! 用途: チェックポイント、設定、プレイヤー進行状況。

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{ThrustError, ThrustResult};

/// セーブデータ全体 (キー → JSON 値)
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub fields: HashMap<String, serde_json::Value>,
}

impl SaveData {
    pub fn new(version: u32) -> Self {
        Self {
            version,
            fields: HashMap::new(),
        }
    }

    /// フィールドに値を保存
    pub fn set<T: Serialize>(&mut self, key: &str, value: &T) -> ThrustResult<()> {
        let json = serde_json::to_value(value)
            .map_err(|e| ThrustError::SceneSerialize(format!("値のシリアライズ失敗: {e}")))?;
        self.fields.insert(key.to_string(), json);
        Ok(())
    }

    /// フィールドから値を読み込む
    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &str) -> ThrustResult<Option<T>> {
        match self.fields.get(key) {
            Some(v) => {
                let parsed = serde_json::from_value(v.clone()).map_err(|e| {
                    ThrustError::SceneSerialize(format!("値のデシリアライズ失敗: {e}"))
                })?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// 既存フィールドかチェック
    pub fn has(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    /// フィールドを削除
    pub fn remove(&mut self, key: &str) {
        self.fields.remove(key);
    }

    /// ファイルに保存
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> ThrustResult<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ThrustError::SceneSerialize(format!("save serialize: {e}")))?;
        std::fs::write(path.as_ref(), json).map_err(|e| ThrustError::Io {
            path: path.as_ref().to_path_buf(),
            source: e,
        })?;
        Ok(())
    }

    /// ファイルからロード
    pub fn load_from_file(path: impl AsRef<Path>) -> ThrustResult<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let content = std::fs::read_to_string(&path_buf).map_err(|e| ThrustError::Io {
            path: path_buf.clone(),
            source: e,
        })?;
        let data: Self = serde_json::from_str(&content)
            .map_err(|e| ThrustError::SceneSerialize(format!("save deserialize: {e}")))?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let mut s = SaveData::new(1);
        s.set("hp", &100_i32).unwrap();
        let v: Option<i32> = s.get("hp").unwrap();
        assert_eq!(v, Some(100));
    }

    #[test]
    fn test_get_missing() {
        let s = SaveData::new(1);
        let v: ThrustResult<Option<i32>> = s.get("missing");
        assert!(v.unwrap().is_none());
    }

    #[test]
    fn test_set_complex() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Player {
            name: String,
            level: u32,
            position: [f32; 3],
        }
        let p = Player {
            name: "Hero".to_string(),
            level: 5,
            position: [1.0, 2.0, 3.0],
        };
        let mut s = SaveData::new(1);
        s.set("player", &p).unwrap();
        let loaded: Player = s.get("player").unwrap().unwrap();
        assert_eq!(loaded, p);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp = std::env::temp_dir().join("thrust_save_test.json");
        let mut s = SaveData::new(2);
        s.set("score", &12345_i64).unwrap();
        s.set("flag", &true).unwrap();
        s.save_to_file(&temp).unwrap();

        let loaded = SaveData::load_from_file(&temp).unwrap();
        assert_eq!(loaded.version, 2);
        let score: i64 = loaded.get("score").unwrap().unwrap();
        assert_eq!(score, 12345);
        let flag: bool = loaded.get("flag").unwrap().unwrap();
        assert!(flag);

        std::fs::remove_file(&temp).ok();
    }

    #[test]
    fn test_has_and_remove() {
        let mut s = SaveData::new(1);
        s.set("k", &"v".to_string()).unwrap();
        assert!(s.has("k"));
        s.remove("k");
        assert!(!s.has("k"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = SaveData::load_from_file("/nonexistent/path/test.json");
        assert!(result.is_err());
    }
}
