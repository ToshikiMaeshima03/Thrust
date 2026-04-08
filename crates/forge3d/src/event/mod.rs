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
        queue.downcast_mut::<Vec<T>>().unwrap().push(event);
    }

    /// 指定した型のイベントをすべて読み取る（消費しない）
    pub fn read<T: 'static>(&self) -> &[T] {
        let type_id = TypeId::of::<T>();
        match self.queues.get(&type_id) {
            Some(queue) => queue.downcast_ref::<Vec<T>>().unwrap(),
            None => &[],
        }
    }

    /// 指定した型のイベントが存在するかチェック
    pub fn has<T: 'static>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.queues
            .get(&type_id)
            .is_some_and(|q| !q.downcast_ref::<Vec<T>>().unwrap().is_empty())
    }

    /// 全イベントキューをクリアする（フレーム末尾で呼び出す）
    pub fn clear(&mut self) {
        self.queues.clear();
    }
}
