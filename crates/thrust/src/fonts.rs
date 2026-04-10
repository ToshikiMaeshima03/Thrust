//! 日本語フォントローダー (Round 9)
//!
//! egui にシステム日本語フォントを読み込む。OS ごとに優先度順で複数のパスを試し、
//! 見つかったフォントを `FontDefinitions` に追加する。
//!
//! どれも見つからない場合は警告ログを出して、egui のデフォルトフォントで継続する。
//! その場合 ASCII 以外の文字 (日本語含む) は豆腐 (□) で表示される。
//!
//! ユーザーが任意のフォントを使いたい場合は `install_font_from_path()` を呼ぶ。

use std::sync::Arc;

/// 試行するシステム日本語フォントパス (優先度順)
const CANDIDATE_PATHS: &[&str] = &[
    // Linux: Noto Sans CJK JP (一般的)
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    // Linux: IPA Gothic (Debian/Ubuntu の japanese-fonts パッケージ)
    "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
    "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
    // Linux: VL Gothic / Takao Gothic
    "/usr/share/fonts/truetype/takao-gothic/TakaoGothic.ttf",
    "/usr/share/fonts/truetype/vlgothic/VL-Gothic-Regular.ttf",
    // macOS: Hiragino Sans
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
    "/Library/Fonts/Hiragino Sans GB.ttc",
    // macOS: Apple Gothic
    "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    // Windows: Yu Gothic
    "C:\\Windows\\Fonts\\YuGothM.ttc",
    "C:\\Windows\\Fonts\\YuGothR.ttc",
    "C:\\Windows\\Fonts\\msgothic.ttc",
    // Windows: Meiryo
    "C:\\Windows\\Fonts\\meiryo.ttc",
];

/// 日本語フォントを読み込んで egui コンテキストにインストールする
///
/// 自動でシステムの日本語フォントを探す。見つからない場合は warning ログを出して
/// false を返すので、ユーザーは手動でフォントを指定できる。
pub fn install_japanese_fonts(ctx: &egui::Context) -> bool {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded = false;

    for path in CANDIDATE_PATHS {
        if let Ok(bytes) = std::fs::read(path) {
            log::info!("日本語フォント読み込み: {path}");
            let font_data = egui::FontData::from_owned(bytes);
            fonts
                .font_data
                .insert("japanese".to_owned(), Arc::new(font_data));

            // Proportional/Monospace 両方の先頭に追加
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, "japanese".to_owned());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.push("japanese".to_owned());
            }
            loaded = true;
            break;
        }
    }

    if loaded {
        ctx.set_fonts(fonts);
    } else {
        log::warn!(
            "日本語フォントが見つかりませんでした。\n  Linux: sudo apt install fonts-noto-cjk または fonts-ipafont-gothic\n  または install_font_from_path() で任意の .ttf を指定してください"
        );
    }
    loaded
}

/// ユーザー指定のパスからフォントを読み込んで egui に登録する
pub fn install_font_from_path(ctx: &egui::Context, path: &str) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    let mut fonts = egui::FontDefinitions::default();
    let font_data = egui::FontData::from_owned(bytes);
    fonts
        .font_data
        .insert("user_font".to_owned(), Arc::new(font_data));
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, "user_font".to_owned());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push("user_font".to_owned());
    }
    ctx.set_fonts(fonts);
    log::info!("カスタムフォント読み込み: {path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_paths_not_empty() {
        assert!(!CANDIDATE_PATHS.is_empty());
    }

    #[test]
    fn test_at_least_some_known_paths() {
        // 各 OS の代表的なパスが含まれているか
        let combined = CANDIDATE_PATHS.join("\n");
        assert!(combined.contains("ipag") || combined.contains("Noto"));
        assert!(combined.contains("Hiragino") || combined.contains("AppleSD"));
        assert!(combined.contains("YuGoth") || combined.contains("msgothic"));
    }

    #[test]
    fn test_install_with_invalid_path() {
        let ctx = egui::Context::default();
        let result = install_font_from_path(&ctx, "/nonexistent/font.ttf");
        assert!(result.is_err());
    }
}
