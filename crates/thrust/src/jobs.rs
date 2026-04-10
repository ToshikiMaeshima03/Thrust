//! ジョブシステム (Round 8)
//!
//! Rayon ベースのデータ並列ヘルパー。
//! 大量のエンティティやパーティクル、地形チャンクの並列処理に使う。
//!
//! 公開する API は薄く、`rayon` 自体を `pub use` することでユーザーが直接使える。

pub use rayon;

use rayon::prelude::*;

/// 並列 for_each ヘルパー
pub fn parallel_for<T: Send + Sync, F: Fn(&T) + Send + Sync>(items: &[T], f: F) {
    items.par_iter().for_each(f);
}

/// 並列 map ヘルパー
pub fn parallel_map<T: Send + Sync, U: Send, F: Fn(&T) -> U + Send + Sync>(
    items: &[T],
    f: F,
) -> Vec<U> {
    items.par_iter().map(f).collect()
}

/// 並列 reduce ヘルパー
pub fn parallel_sum<T: Send + Sync + Copy, F: Fn(&T) -> f32 + Send + Sync>(
    items: &[T],
    f: F,
) -> f32 {
    items.par_iter().map(f).sum()
}

/// 範囲 (0..n) を並列に実行する
pub fn parallel_range<F: Fn(usize) + Send + Sync>(n: usize, f: F) {
    (0..n).into_par_iter().for_each(f);
}

/// グローバルスレッドプールのスレッド数を取得
pub fn num_threads() -> usize {
    rayon::current_num_threads()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_parallel_for_visits_all() {
        let items = vec![1, 2, 3, 4, 5];
        let counter = AtomicUsize::new(0);
        parallel_for(&items, |i| {
            counter.fetch_add(*i, Ordering::Relaxed);
        });
        assert_eq!(counter.load(Ordering::Relaxed), 15);
    }

    #[test]
    fn test_parallel_map() {
        let items = vec![1, 2, 3, 4];
        let mut squared = parallel_map(&items, |x| x * x);
        squared.sort();
        assert_eq!(squared, vec![1, 4, 9, 16]);
    }

    #[test]
    fn test_parallel_sum() {
        let items: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let sum = parallel_sum(&items, |x| *x);
        assert!((sum - 499500.0).abs() < 1.0);
    }

    #[test]
    fn test_parallel_range() {
        let counter = AtomicUsize::new(0);
        parallel_range(100, |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_num_threads_positive() {
        assert!(num_threads() > 0);
    }
}
