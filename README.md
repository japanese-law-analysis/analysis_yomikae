[![Workflow Status](https://github.com/japanese-law-analysis/analysis_yomikae/workflows/Rust%20CI/badge.svg)](https://github.com/japanese-law-analysis/analysis_yomikae/actions?query=workflow%3A%22Rust%2BCI%22)

# analysis_yomikae

読み替え文を解析し、読み替えられる対象の文言と、読み替え後の文言を取り出すソフトウェアです。


## CLIソフトウェアを使う

実際の法令XMLファイルに対して読み替えの文の解析を行って、情報を取得するソフトウェアです。

### インストール

```sh
cargo install --git "https://github.com/japanese-law-analysis/analysis_yomikae.git"
```

### 使い方

```sh
analysis_yomikae -o output.json -e err.json -w law_xml -i index.json
```

で起動します。

オプションの各意味は以下のとおりです。

- `-o`：解析で生成した情報を出力するJSONファイル
- `-e`：解析に失敗した条文の情報を出力するJSONファイル
- `-w`：法令XMLファイルがあるフォルダ
- `-i`：法令のインデックス情報が書かれたJSONファイル [listup_law](https://github.com/japanese-law-analysis/listup_law)で生成するもの


## ライブラリを使う
詳しくはリポジトリを手元にクローンした上で`cargo doc --open`でドキュメントを生成してください。

解析結果が書かれたJSONファイルに書かれる構造体やエラーの定義がされており、デシリアライズが容易にできるようになっています。


---

[MIT License](https://github.com/japanese-law-analysis/analysis_yomikae/blob/master/LICENSE)
(c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")


License: MIT
