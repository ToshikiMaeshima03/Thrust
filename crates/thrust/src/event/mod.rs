use std::any::{Any, TypeId};
use std::collections::HashMap;

/// 型消去イベントキュー
///
/// システム間でデータを受け渡すためのフレーム単位のメッセージングシステム。
/// イベントは `send()` で送信し、`read()` で受信する。フレーム末尾で `clear()` される。
pub struct Events {
    queues: HashMap<TypeId, Box<dyn Any>>,
}

impl Default for Events {
    fn default() -> Self {
        Self::new()
    }
}

impl Events {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
        }
    }

    /// イベントを送信する
    pub fn send<T: 'static>(&mut self, event: T) {
        let type_id = TypeId::of::<T>();
        let queue = self
            .queues
            .entry(type_id)
            .or_insert_with(|| Box::new(Vec::<T>::new()));
        queue
            .downcast_mut::<Vec<T>>()
            .expect("内部エラー: イベントキューの型が不一致です")
            .push(event);
    }

    /// 指定した型のイベントをすべて読み取る（消費しない）
    pub fn read<T: 'static>(&self) -> &[T] {
        let type_id = TypeId::of::<T>();
        match self.queues.get(&type_id) {
            Some(queue) => queue
                .downcast_ref::<Vec<T>>()
                .expect("内部エラー: イベントキューの型が不一致です"),
            None => &[],
        }
    }

    /// 指定した型のイベントが存在するかチェック
    pub fn has<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.queues.get(&type_id).is_some_and(|q| {
            !q.downcast_ref::<Vec<T>>()
                .expect("内部エラー: イベントキューの型が不一致です")
                .is_empty()
        })
    }

    /// 全イベントキューをクリアする（フレーム末尾で呼び出す）
    pub fn clear(&mut self) {
        self.queues.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct DamageEvent {
        amount: u32,
    }

    #[derive(Debug, PartialEq)]
    struct HealEvent {
        amount: u32,
    }

    #[test]
    fn test_send_and_read() {
        let mut events = Events::new();
        events.send(DamageEvent { amount: 10 });
        events.send(DamageEvent { amount: 20 });

        let damages = events.read::<DamageEvent>();
        assert_eq!(damages.len(), 2);
        assert_eq!(damages[0].amount, 10);
        assert_eq!(damages[1].amount, 20);
    }

    #[test]
    fn test_read_empty() {
        let events = Events::new();
        let damages = events.read::<DamageEvent>();
        assert!(damages.is_empty());
    }

    #[test]
    fn test_has() {
        let mut events = Events::new();
        assert!(!events.has::<DamageEvent>());

        events.send(DamageEvent { amount: 5 });
        assert!(events.has::<DamageEvent>());
        assert!(!events.has::<HealEvent>());
    }

    #[test]
    fn test_multiple_types() {
        let mut events = Events::new();
        events.send(DamageEvent { amount: 10 });
        events.send(HealEvent { amount: 5 });

        assert_eq!(events.read::<DamageEvent>().len(), 1);
        assert_eq!(events.read::<HealEvent>().len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut events = Events::new();
        events.send(DamageEvent { amount: 10 });
        events.send(HealEvent { amount: 5 });

        events.clear();
        assert!(!events.has::<DamageEvent>());
        assert!(!events.has::<HealEvent>());
        assert!(events.read::<DamageEvent>().is_empty());
    }

    #[test]
    fn test_read_does_not_consume() {
        let mut events = Events::new();
        events.send(DamageEvent { amount: 10 });

        // 2回読んでも同じ結果
        assert_eq!(events.read::<DamageEvent>().len(), 1);
        assert_eq!(events.read::<DamageEvent>().len(), 1);
    }
}
