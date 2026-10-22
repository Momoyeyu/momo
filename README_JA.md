<p align="center">
  <img src="docs/assets/momo-readme-banner.svg" width="100%" alt="momo：墨紫の夜に浮かぶ切り紙と星の軌跡">
</p>

<h1 align="center">momo</h1>
<p align="center"><strong>ここで、少しずつ。お互いを知っていく。</strong><br>一緒に過ごす時間と記憶、少しずつ深まる関係を目指すAIパートナーのゲーム。</p>

<p align="center">
  <a href="README.md">简体中文</a> · <a href="README_EN.md">English</a> · <strong>日本語</strong>
</p>
<p align="center">
  <a href="https://momoyeyu.github.io/momo/ja/">momoに会う</a> ·
  <a href="docs/usage/getting-started.md">はじめ方</a> ·
  <a href="https://github.com/Momoyeyu/momo/releases">リリース状況</a> ·
  <a href="CONTRIBUTING.md">一緒につくる</a>
</p>
<p align="center">
  <a href="https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml"><img src="https://github.com/Momoyeyu/momo/actions/workflows/ci-app.yml/badge.svg?branch=master" alt="アプリの検証状況"></a>
  <a href="https://github.com/Momoyeyu/momo/actions/workflows/ci-site.yml"><img src="https://github.com/Momoyeyu/momo/actions/workflows/ci-site.yml/badge.svg?branch=master" alt="ウェブサイトの検証状況"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/code-MIT-E7BE77?labelColor=28232E" alt="プロジェクトのコード：MITライセンス"></a>
</p>

## 「今日はどうだった？」から始めよう

気の利いた質問を用意しなくても大丈夫。今日あったことをmomoに話し、返事を聞いて、表情や動きを眺める。次に帰ってきたとき、名前や好み、一緒に話した小さなことを記憶に残せます。

momoには一貫した核となる性格があり、一緒に過ごした経験のための余白もあります。会話を振り返り、セーブを残し、設定を変え、何を覚えていてほしいかを選べる。ゆっくりと馴染んでいけるゲームを目指しています。

<p align="center">
  <a href="https://momoyeyu.github.io/momo/ja/"><img src="site/docs/public/media/first-meeting.png" width="100%" alt="momoでの実際の初対面の画面。暖かい部屋の中央にひよりが立ち、どのように呼べばよいか尋ねています。"></a>
</p>
<p align="center"><a href="https://momoyeyu.github.io/momo/ja/">実際の無音デモ映像を見て、一緒のひとときをめくってみよう</a></p>

<sub>画面には隔離したデモデータとHiyoriサンプルモデルを使用し、個人の会話や設定は含みません。[素材の出典と説明](site/docs/public/media/asset-notice.txt)。</sub>

## いま、一緒にできること

| 体験 | 現在の内容 |
| --- | --- |
| **はじめまして** | 挨拶と名前を尋ねるところから、あなた自身の会話へ。 |
| **文字でも、マイクでも** | 文字を打つことも、話しかけることも。音声認識はアプリに同梱されています。 |
| **表情のある返事** | Live2Dの姿、表情と動き。任意でGPT-SoVITSを接続し、音声での返事、中断と再開も。 |
| **自分で選べる記憶** | 記憶を確認・編集・削除し、間違って覚えた小さなことも修正できます。 |
| **残すこと、持ち出すこと** | 会話履歴、セーブとロード、データの書き出し・読み込みで、復元できる記録を。 |
| **あなたらしい過ごし方** | 姿や動きの変更、中文 / English / 日本語の切り替え、信頼する拡張ツールの接続。 |

長く一緒に過ごす時間をより自然にし、共有した経験が会話に繊細な違いを生むことは、これからも磨いていく方向です。いま体験できる内容は、リリース説明をご確認ください。

## はじめて会うには

**macOSの候補版では、インストーラーと実際の音声連携の検証が完了しています。公開済みのインストーラーはまだありません。** 配布状況は[リリースページ](https://github.com/Momoyeyu/momo/releases)、ソースから動かす場合は[開発ガイド](docs/development/migration.md)をご覧ください。

体験版を入手したら、まずは三つの小さなことから。

1. 設定に互換性のある会話サービスのURL、APIキー、モデルを指定し、接続を確認します。
2. 文字かマイクで挨拶してみましょう。文字の会話に音声合成サービスの構築は必要ありません。
3. 声での返事を聞きたいときは、任意のTTSを接続し、利用する権利のある参考音声と、その内容を書き起こした文章を設定します。

会話は選択したモデルサービスへ送信されます。音声合成を有効にした場合、参考音声も選択した音声サービスへ送信されます。標準の声のサンプルは付属しません。外部サービスの料金と規約は、各サービスによって定められます。

| 知りたいこと | 案内 |
| --- | --- |
| 初回起動、会話モデルと姿の設定 | [はじめ方](docs/usage/getting-started.md) |
| 参考音声、音声の接続、任意のGPU構築 | [音声の設定](docs/usage/tts.md) |
| 記録の保存先、バックアップと移行 | [バックアップと移行](docs/usage/data.md) |
| ソースからの開発・実行・ビルド | [開発ガイド](docs/development/migration.md) · [実行時の設定](docs/development/configuration.md) |

詳しいガイド本文は現在、中国語の原文です。ウェブサイトとアプリは、中国語・英語・日本語に対応しています。

現在は **macOS** の体験を優先しています。**Windows** は今後別途進め、**Linux** は二次的・任意の方向としています。パソコン向けで、スマートフォン・タブレット版は提供していません。

## ここを、一緒に育てる

コード以外にもできることがあります。具体的な体験の報告、自然な翻訳、姿や操作のデザインについての提案も、一緒の時間をよりよくしてくれます。

- **試して、伝える**：[不具合やアイデアを報告](https://github.com/Momoyeyu/momo/issues)し、何が起きたか、どのような体験を期待したかを教えてください。
- **文章・翻訳・デザイン**：中国語・英語・日本語の表現の改善や、話し合える画面案を歓迎します。
- **コードとツール**：画面、記憶の体験、音声、信頼できる拡張など。[貢献ガイド](CONTRIBUTING.md)から開発と検証の案内へ進めます。

## 謝辞とライセンス

AIキャラクターや一緒に過ごす体験に取り組む[AIRI](https://github.com/moeru-ai/airi)などのオープンソースプロジェクトに感謝します。標準のHiyoriモデルはLive2D Inc.によるものです。フォント、音声認識、検索モデル、音声サービスも、それぞれの上流プロジェクトの成果を利用しています。

プロジェクトのコードは[MITライセンス](LICENSE.md)で提供します。外部のモデル、Cubism Core、フォント、依存関係、紹介素材には、それぞれの利用条件が適用されます。リポジトリのMITライセンスで置き換わるものではありません。出典、ライセンス、再配布に関する注意事項は[第三者ライセンスの説明](THIRD_PARTY_NOTICES.md)をご覧ください。
