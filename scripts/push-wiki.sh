#!/bin/bash
# GitHub Wiki ページをプッシュするスクリプト
#
# 使い方:
#   1. https://github.com/ToshikiMaeshima03/Thrust/wiki で
#      「Create the first page」をクリックして初期ページを作成
#   2. このスクリプトを実行
#
# 前提: gh auth login 済み、/tmp/thrust-wiki にコンテンツが存在

set -euo pipefail

WIKI_DIR="/tmp/thrust-wiki"
REPO_URL="https://github.com/ToshikiMaeshima03/Thrust.wiki.git"

if [ ! -d "$WIKI_DIR" ]; then
    echo "エラー: $WIKI_DIR が存在しません"
    exit 1
fi

# 既存の wiki をクローンしてコンテンツを上書き
WORK_DIR=$(mktemp -d)
git clone "$REPO_URL" "$WORK_DIR/wiki"

# 既存ファイルを削除してから新しいコンテンツをコピー
find "$WORK_DIR/wiki" -maxdepth 1 -name '*.md' -delete
cp "$WIKI_DIR"/*.md "$WORK_DIR/wiki/"

cd "$WORK_DIR/wiki"
git add -A
git commit -m "Wiki: 初期ページ作成 (Home, Getting Started, Architecture, API Reference, Shader Guide, Contributing)" || {
    echo "変更なし — Wiki は最新です"
    exit 0
}
git push

echo "Wiki をプッシュしました"
echo "https://github.com/ToshikiMaeshima03/Thrust/wiki"

# クリーンアップ
rm -rf "$WORK_DIR"
