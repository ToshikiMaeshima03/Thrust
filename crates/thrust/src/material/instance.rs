//! マテリアルインスタンス (Round 6)
//!
//! UE 風の Material Instance: ベースマテリアル (`MaterialTemplate`) を共有し、
//! インスタンスごとにスカラー/カラー パラメータを override する。
//!
//! 複数のオブジェクトが同じベース材質から派生する場合に:
//! - ベースのテクスチャ・設定を共有
//! - インスタンスごとの base_color や metallic 等を個別に設定
//! - `resolve()` で最終的な `Material` を構築

use std::collections::HashMap;
use std::sync::Arc;

use crate::material::material::Material;
use crate::renderer::texture::ThrustTexture;

/// マテリアルテンプレート (ベース)
///
/// テクスチャとデフォルトパラメータを保持する。
#[derive(Clone)]
pub struct MaterialTemplate {
    pub base: Material,
    /// 名前付きスカラー パラメータ (0..1)
    pub scalar_params: HashMap<String, f32>,
    /// 名前付きカラー パラメータ
    pub color_params: HashMap<String, glam::Vec4>,
}

impl MaterialTemplate {
    pub fn new(base: Material) -> Self {
        Self {
            base,
            scalar_params: HashMap::new(),
            color_params: HashMap::new(),
        }
    }

    pub fn with_scalar(mut self, key: impl Into<String>, value: f32) -> Self {
        self.scalar_params.insert(key.into(), value);
        self
    }

    pub fn with_color(mut self, key: impl Into<String>, value: glam::Vec4) -> Self {
        self.color_params.insert(key.into(), value);
        self
    }

    /// インスタンスを作成
    pub fn instance(self: &Arc<Self>) -> MaterialInstance {
        MaterialInstance {
            template: self.clone(),
            scalar_overrides: HashMap::new(),
            color_overrides: HashMap::new(),
        }
    }
}

/// マテリアルインスタンス: テンプレートを参照し、パラメータを上書きする
pub struct MaterialInstance {
    pub template: Arc<MaterialTemplate>,
    pub scalar_overrides: HashMap<String, f32>,
    pub color_overrides: HashMap<String, glam::Vec4>,
}

impl MaterialInstance {
    /// 最終的なスカラー値を取得 (override → template → 0.0)
    pub fn get_scalar(&self, key: &str) -> f32 {
        self.scalar_overrides
            .get(key)
            .copied()
            .or_else(|| self.template.scalar_params.get(key).copied())
            .unwrap_or(0.0)
    }

    /// 最終的なカラー値を取得
    pub fn get_color(&self, key: &str) -> glam::Vec4 {
        self.color_overrides
            .get(key)
            .copied()
            .or_else(|| self.template.color_params.get(key).copied())
            .unwrap_or(glam::Vec4::ZERO)
    }

    /// パラメータを override する
    pub fn set_scalar(&mut self, key: impl Into<String>, value: f32) {
        self.scalar_overrides.insert(key.into(), value);
    }

    pub fn set_color(&mut self, key: impl Into<String>, value: glam::Vec4) {
        self.color_overrides.insert(key.into(), value);
    }

    /// パラメータ override を解除 (テンプレート値に戻す)
    pub fn reset_scalar(&mut self, key: &str) {
        self.scalar_overrides.remove(key);
    }

    pub fn reset_color(&mut self, key: &str) {
        self.color_overrides.remove(key);
    }

    /// インスタンスから最終的な Material を構築する
    ///
    /// テンプレートの Material をクローンしてから、予約済みパラメータキー
    /// (`base_color`, `metallic`, `roughness`, `emissive`) で上書きする。
    pub fn resolve(&self) -> Material {
        let mut mat = self.template.base.clone();

        // 予約済みキー
        if self.scalar_overrides.contains_key("metallic")
            || self.template.scalar_params.contains_key("metallic")
        {
            mat.metallic_factor = self.get_scalar("metallic");
        }
        if self.scalar_overrides.contains_key("roughness")
            || self.template.scalar_params.contains_key("roughness")
        {
            mat.roughness_factor = self.get_scalar("roughness");
        }
        if self.scalar_overrides.contains_key("normal_scale")
            || self.template.scalar_params.contains_key("normal_scale")
        {
            mat.normal_scale = self.get_scalar("normal_scale");
        }
        if self.color_overrides.contains_key("base_color")
            || self.template.color_params.contains_key("base_color")
        {
            mat.base_color_factor = self.get_color("base_color");
        }
        if self.color_overrides.contains_key("emissive")
            || self.template.color_params.contains_key("emissive")
        {
            let e = self.get_color("emissive");
            mat.emissive_factor = glam::Vec3::new(e.x, e.y, e.z);
        }

        mat
    }

    /// base_color_map を差し替える (新規インスタンスを作成)
    pub fn with_base_color_map(self, _tex: Arc<ThrustTexture>) -> Self {
        // テクスチャ override が必要な場合は MaterialInstance に Option<Arc<_>> フィールドを追加すべき
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_new() {
        let tpl = MaterialTemplate::new(Material::default());
        assert!(tpl.scalar_params.is_empty());
    }

    #[test]
    fn test_template_with_scalar() {
        let tpl = MaterialTemplate::new(Material::default())
            .with_scalar("metallic", 0.5)
            .with_scalar("roughness", 0.3);
        assert!((tpl.scalar_params["metallic"] - 0.5).abs() < 1e-5);
        assert!((tpl.scalar_params["roughness"] - 0.3).abs() < 1e-5);
    }

    #[test]
    fn test_instance_inherits_template() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()).with_scalar("metallic", 0.7));
        let inst = tpl.instance();
        assert!((inst.get_scalar("metallic") - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_instance_override() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()).with_scalar("metallic", 0.7));
        let mut inst = tpl.instance();
        inst.set_scalar("metallic", 1.0);
        assert!((inst.get_scalar("metallic") - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_instance_reset() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()).with_scalar("metallic", 0.7));
        let mut inst = tpl.instance();
        inst.set_scalar("metallic", 1.0);
        inst.reset_scalar("metallic");
        // テンプレート値に戻る
        assert!((inst.get_scalar("metallic") - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_resolve_applies_overrides() {
        let tpl = Arc::new(
            MaterialTemplate::new(Material::default())
                .with_scalar("metallic", 0.5)
                .with_scalar("roughness", 0.3),
        );
        let mut inst = tpl.instance();
        inst.set_scalar("metallic", 1.0);
        let mat = inst.resolve();
        assert!((mat.metallic_factor - 1.0).abs() < 1e-5);
        assert!((mat.roughness_factor - 0.3).abs() < 1e-5);
    }

    #[test]
    fn test_resolve_color_override() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()));
        let mut inst = tpl.instance();
        inst.set_color("base_color", glam::Vec4::new(1.0, 0.0, 0.0, 1.0));
        let mat = inst.resolve();
        assert!((mat.base_color_factor.x - 1.0).abs() < 1e-5);
        assert!((mat.base_color_factor.y - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_multiple_instances_share_template() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()));
        let _inst1 = tpl.instance();
        let _inst2 = tpl.instance();
        // Arc の参照カウントが増える
        assert_eq!(Arc::strong_count(&tpl), 3);
    }

    #[test]
    fn test_missing_param_returns_zero() {
        let tpl = Arc::new(MaterialTemplate::new(Material::default()));
        let inst = tpl.instance();
        assert_eq!(inst.get_scalar("unknown"), 0.0);
    }
}
