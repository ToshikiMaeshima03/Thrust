//! Rhai スクリプティング統合 (Round 7)
//!
//! 軽量埋め込みスクリプト言語 Rhai を使い、エンジン外からゲームロジックを記述できる。
//! Transform/Vec3/Time などの型を Rhai に公開する。
//!
//! ## 使用例
//! ```ignore
//! use thrust::scripting::ScriptEngine;
//! let mut engine = ScriptEngine::new();
//! engine.eval(r#"
//!     let v = vec3(1.0, 2.0, 3.0);
//!     let len = v.length();
//!     print(`length = ${len}`);
//! "#).unwrap();
//! ```

use std::sync::Arc;

use glam::Vec3;
use rhai::{AST, Engine, Scope};

use crate::error::{ThrustError, ThrustResult};

/// Rhai スクリプトエンジンのラッパー
///
/// `Engine` + `Scope` をひとまとめに保持し、`Vec3` / Math ヘルパ / `print` などを
/// 自動登録する。
pub struct ScriptEngine {
    pub engine: Engine,
    pub scope: Scope<'static>,
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        register_glam(&mut engine);
        register_helpers(&mut engine);
        Self {
            engine,
            scope: Scope::new(),
        }
    }

    /// 式を評価して結果を返す
    pub fn eval<T: Clone + Send + Sync + 'static>(&mut self, source: &str) -> ThrustResult<T> {
        self.engine
            .eval_with_scope::<T>(&mut self.scope, source)
            .map_err(|e| ThrustError::Script(format!("{e}")))
    }

    /// スコープ環境を保持したままスクリプトを実行する (`run`)
    pub fn run(&mut self, source: &str) -> ThrustResult<()> {
        self.engine
            .run_with_scope(&mut self.scope, source)
            .map_err(|e| ThrustError::Script(format!("{e}")))
    }

    /// AST を事前コンパイルする (毎フレーム再評価する場合に有効)
    pub fn compile(&mut self, source: &str) -> ThrustResult<AST> {
        self.engine
            .compile(source)
            .map_err(|e| ThrustError::Script(format!("compile error: {e}")))
    }

    /// 事前コンパイル済み AST を実行する (`run_ast`)
    pub fn run_ast(&mut self, ast: &AST) -> ThrustResult<()> {
        self.engine
            .run_ast_with_scope(&mut self.scope, ast)
            .map_err(|e| ThrustError::Script(format!("{e}")))
    }

    /// グローバル変数を設定する
    pub fn set_var<T: Clone + Send + Sync + 'static>(&mut self, name: &str, value: T) {
        self.scope.push(name.to_string(), value);
    }

    /// グローバル変数を取得する
    pub fn get_var<T: Clone + Send + Sync + 'static>(&self, name: &str) -> Option<T> {
        self.scope.get_value::<T>(name)
    }
}

/// glam の Vec3 を Rhai に登録する
fn register_glam(engine: &mut Engine) {
    engine
        .register_type_with_name::<Vec3>("Vec3")
        .register_fn("vec3", |x: f64, y: f64, z: f64| {
            Vec3::new(x as f32, y as f32, z as f32)
        })
        .register_fn("vec3_zero", || Vec3::ZERO)
        .register_fn("vec3_one", || Vec3::ONE)
        .register_get("x", |v: &mut Vec3| v.x as f64)
        .register_get("y", |v: &mut Vec3| v.y as f64)
        .register_get("z", |v: &mut Vec3| v.z as f64)
        .register_set("x", |v: &mut Vec3, n: f64| v.x = n as f32)
        .register_set("y", |v: &mut Vec3, n: f64| v.y = n as f32)
        .register_set("z", |v: &mut Vec3, n: f64| v.z = n as f32)
        .register_fn("length", |v: &mut Vec3| v.length() as f64)
        .register_fn("length_squared", |v: &mut Vec3| v.length_squared() as f64)
        .register_fn("normalize", |v: &mut Vec3| v.normalize_or_zero())
        .register_fn("dot", |a: Vec3, b: Vec3| a.dot(b) as f64)
        .register_fn("cross", |a: Vec3, b: Vec3| a.cross(b))
        .register_fn("+", |a: Vec3, b: Vec3| a + b)
        .register_fn("-", |a: Vec3, b: Vec3| a - b)
        .register_fn("*", |v: Vec3, s: f64| v * s as f32)
        .register_fn("*", |s: f64, v: Vec3| v * s as f32)
        .register_fn("to_string", |v: &mut Vec3| {
            format!("vec3({}, {}, {})", v.x, v.y, v.z)
        });
}

/// 数学ヘルパや print を登録する
fn register_helpers(engine: &mut Engine) {
    engine
        .register_fn("lerp", |a: f64, b: f64, t: f64| a + (b - a) * t)
        .register_fn("smoothstep", |a: f64, b: f64, t: f64| {
            let t = ((t - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .register_fn("clamp", |x: f64, lo: f64, hi: f64| x.max(lo).min(hi))
        .register_fn("deg_to_rad", |d: f64| d.to_radians())
        .register_fn("rad_to_deg", |r: f64| r.to_degrees());
}

/// 共有スクリプトハンドル (複数システムで使い回す)
pub type SharedScript = Arc<std::sync::Mutex<ScriptEngine>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_int() {
        let mut e = ScriptEngine::new();
        let result: i64 = e.eval("1 + 2").unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_eval_float() {
        let mut e = ScriptEngine::new();
        let result: f64 = e.eval("1.5 * 2.0").unwrap();
        assert!((result - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_vec3_construction() {
        let mut e = ScriptEngine::new();
        let v: Vec3 = e.eval("vec3(1.0, 2.0, 3.0)").unwrap();
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_vec3_length() {
        let mut e = ScriptEngine::new();
        let len: f64 = e.eval("vec3(3.0, 4.0, 0.0).length()").unwrap();
        assert!((len - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_vec3_addition() {
        let mut e = ScriptEngine::new();
        let v: Vec3 = e.eval("vec3(1.0, 0.0, 0.0) + vec3(0.0, 1.0, 0.0)").unwrap();
        assert_eq!(v, Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn test_lerp_helper() {
        let mut e = ScriptEngine::new();
        let v: f64 = e.eval("lerp(0.0, 10.0, 0.5)").unwrap();
        assert!((v - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_set_get_var() {
        let mut e = ScriptEngine::new();
        e.set_var("hp", 100_i64);
        let v: i64 = e.eval("hp + 50").unwrap();
        assert_eq!(v, 150);
    }

    #[test]
    fn test_compile_and_run() {
        let mut e = ScriptEngine::new();
        let ast = e.compile("let counter = 5;").unwrap();
        e.run_ast(&ast).unwrap();
        // counter は scope に残る
        let v: i64 = e.eval("counter").unwrap();
        assert_eq!(v, 5);
    }

    #[test]
    fn test_smoothstep_helper() {
        let mut e = ScriptEngine::new();
        let v: f64 = e.eval("smoothstep(0.0, 1.0, 0.5)").unwrap();
        assert!((v - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_script_error() {
        let mut e = ScriptEngine::new();
        let result: ThrustResult<i64> = e.eval("undefined_function()");
        assert!(result.is_err());
    }
}
