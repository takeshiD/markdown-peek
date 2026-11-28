// Builds docs/components.md from docs/components/src/*.md and the images in
// docs/components/media/ (made by web/e2e/components.spec.ts).
//
//   node docs/components/build.mjs
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const media = (f) => path.join(here, "media", f);

/** [file id, title, candidate id, what it is for and how it is used] */
const SECTION = [
  ["steps", "Steps", "StepNavigator", "番号付きリスト（3 項目以上）を 1 ステップずつ進めるナビ。前へ / 次へで現在のステップが移り、進捗バーが伸びます。現在のステップは文書ごとにこのブラウザに保存されます。"],
  ["checklist", "Checklist", "Checklist", "タスクリスト（2 項目以上）を操作できるチェックリストにします。元のリストの位置に置き換わり（#63）、*Original* で Markdown の表示に戻せます。チェックはこのブラウザにだけ保存され、ファイルは変更しません。*Reset to file* でファイルどおりに戻ります。"],
  ["table-status", "Table（ステータス）", "DataTable:status", "Status / 状態 などの列がある表を、ステータスを追える表にします。元の表の位置に置き換わります。"],
  ["table-sortable", "Table（並べ替え・絞り込み）", "DataTable:sortable", "3 行以上の表を、列見出しのクリックで並べ替え、文字で行を絞り込める表にします。元の表の位置に置き換わります。"],
  ["chart", "Chart", "Chart", "数値の列がある表をグラフにします（日付・月が並ぶ表は折れ線、それ以外は棒）。値はセルの文字列そのままで、矢印キーやポインタで各点の値を読めます。数値の列が複数あれば切り替えボタンが出ます。"],
  ["flow", "Flow", "NodeGraph", "`A → B` の矢印で書いた流れ（または from / to の表）をノードグラフにします。ノードを選ぶと、どこから来てどこへ行くかと、元の行が出ます。`--PASS-->` のようにラベル付きの分岐は結果ボタンで切り替えられます。"],
  ["flow-diagram", "Flow（罫線の図）", "NodeGraph:diagram", "コードブロックに罫線（`┌─┐ │ ▶`）で描いた箱と矢印の図を、同じノードグラフで描き直します（#62）。読み取れない図からは何も作りません。"],
  ["flow-stages", "Flow（処理の各段）", "NodeGraph:stages", "`### 1. parser — markdown -> BlockTree` のように、小見出しが段階を 1 つずつ説明している節に、段階のフロー図を付けます（#65）。矢印には見出しに書かれた出力が付き、ノードから各小見出しへ移動できます。"],
  ["comparison", "Comparison", "Comparison", "選択肢を列、評価軸を行に並べた点数付きの表を、選択肢ごとのカードにします。評価軸を選ぶとその軸で比べられ、平均の最高点に *Highest* が付きます。点数でない行（所要時間など）はカードの下に出ます。"],
  ["directory-tree", "Directory tree", "DirectoryTree", "`├──` / `└──` で描いたディレクトリ構成を、開閉できるツリーにします（#64）。`#` の後のコメントを表示し、★ の付いた項目を強調します。マウスでもキーボード（矢印キー）でも開閉できます。"],
  ["timeline", "Timeline", "Timeline", "日付やバージョンが付いた行（3 つ以上）を時系列に並べます。"],
  ["callout-risk", "Callout（リスク）", "Callout:risk", "「リスク」「注意」などの見出しの節にある段落を、警告として目立たせます。"],
];

const DOCUMENT = [
  ["doc-outline", "Outline by role", "SemanticOutline", "見出しを役割（概要・設計・決定・リスク・未解決の問い …）ごとにまとめたアウトライン。見出しが 4 つ以上あり、役割が分かれている文書で出ます。"],
  ["doc-risks", "Risks", "RiskPanel", "「リスク」「注意」などの見出しの節に書かれた内容を一覧にします。"],
  ["doc-questions", "Open questions", "OpenQuestions", "「Open Questions」「未解決」「疑問」などの見出しの節に書かれた問いを一覧にします。"],
  ["doc-actions", "Action items", "ActionItems", "文書中のタスクリスト項目と `TODO:` を集め、チェックできる一覧にします。ほとんど完了済みのリスト（機能一覧など）では出ません。"],
  ["doc-decisions", "Decisions", "DecisionLog", "「採用する」「決定」「方針とする」「we decided」などを含む段落を決定事項として集めます。各行の `decided` と `medium` は今は固定の表示で、意味のある情報ではありません（下の注を参照）。"],
];

function source(id) {
  const md = readFileSync(path.join(here, "src", `${id}.md`), "utf8").trimEnd();
  return "````markdown\n" + md + "\n````";
}

function entry([id, title, candidate, text]) {
  const out = [`### ${title}`, "", `候補 id: \`${candidate}\``, "", text, "", source(id), ""];
  out.push(`![${title}](components/media/${id}.png)`, "");
  if (existsSync(media(`${id}.gif`))) out.push(`操作の例:`, "", `![${title} を操作したところ](components/media/${id}.gif)`, "");
  return out.join("\n");
}

const doc = `# GenUI コンポーネント集

GenUI（ブラウザ表示の \`GenUI\` トグル）が文書に入れるコンポーネントの一覧です。どれも**本文から抽出した内容**でできていて、元の行（\`L5–12\` などの行番号）へ戻れます。モデルは、抽出できた候補の中から節ごとに 1 つを選ぶか、何も出さないことを選ぶだけです。中身は書きません（[設計判断](design/llm-generated-aids.md)）。

- **節ごとのコンポーネント**（12 候補・9 種類）は、元になった行の直後に本文の一部として入ります。
- **文書全体のコンポーネント**（5 種類）は、冒頭の節の後に折り畳んで入ります。
- 1 つの文書に出るのは最大 8 個です。
- 各コンポーネントのヘッダには、ⓘ（なぜこれが選ばれたか）、ピン留め、非表示のボタンがあります。

各項目の Markdown は、そのまま \`mdpeek serve\` に渡すと同じコンポーネントになります（ファイルは [\`components/src/\`](components/src/)）。

## 節ごとのコンポーネント

${SECTION.map(entry).join("\n")}
## 文書全体のコンポーネント

${DOCUMENT.map(entry).join("\n")}
## 注

- **Glossary（用語集）** は種類としては残っていますが、今は出ません。略語は見つけられても定義を抽出できず、定義のない用語は出さない決まり（定義を推測で書かない）だからです。
- **Decisions の \`decided\` / \`medium\`**: \`decided\` は決定の状態（ほかに \`proposed\` / \`rejected\` / \`superseded\` がある）、\`medium\` は抽出の確からしさです。ただし今の抽出はキーワードで段落を拾うだけなので、どの行も \`decided\` と \`medium\` に固定されています。「却下」と書いた段落も \`decided\` になります。
- 画像は \`web/e2e/components.spec.ts\` で作ります（\`MDPEEK_COMPONENT_MEDIA=1 npx playwright test e2e/components.spec.ts\`、GIF には ImageMagick が必要）。この文書は \`node docs/components/build.mjs\` で作り直せます。
`;

writeFileSync(path.join(here, "..", "components.md"), doc);
console.log("wrote docs/components.md");
