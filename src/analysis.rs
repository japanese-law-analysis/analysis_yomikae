use anyhow::{anyhow, Result};
use jplaw_text::LawText;
use mecab::Tagger;
use search_article_with_word::Chapter;
use serde::{Deserialize, Serialize};
use tracing::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct YomikaeInfo {
  /// 法律番号
  num: String,
  /// その読み替え規定がある条項
  chapter: Chapter,
  /// 読み替えられる前の単語
  before_words: Vec<String>,
  /// 読み替えられた後の単語
  after_word: String,
}

/// 読み替え規定文は
/// 「((「〜〜」とあり)*「〜〜」とあるのは「〜〜」(と、|と))+読み替えるものとする。」
/// のような形になっている（読点の有無等の違いは微妙にはある）
pub async fn parse_yomikae(
  law_text: &LawText,
  num: &str,
  chapter: &Chapter,
  mecab_ipadic_path: &str,
) -> Result<Vec<YomikaeInfo>> {
  let input = &law_text.contents;
  info!("[INPUT] {num} : {}", input);
  let bytes_of_input = input.as_bytes();

  let mut tagger = Tagger::new(mecab_ipadic_path);
  let mut node = tagger.parse_to_node(bytes_of_input);

  let mut yomikae_info_lst = Vec::new();

  // 角カッコの開き
  let mut open_kakko_depth: usize = 0;
  // 角括弧の中にある文字
  let mut word_in_kakko = String::new();

  let mut before_words = Vec::new();
  let mut is_before_words_end = false;

  loop {
    match node.stat as i32 {
      mecab::MECAB_BOS_NODE => {
        info!("[START [PARSE] {num} : {chapter:?}");
      }
      mecab::MECAB_EOS_NODE => {
        info!("[END PARSE] {num} : {chapter:?}");
        break;
      }
      _ => {
        let word = &(node.surface)[..(node.length as usize)];
        let mut features = node.feature.split(',');
        // 品詞情報
        let hinshi: &str = features.nth(0).unwrap();
        match (word, hinshi) {
          ("「", "記号") => {
            if features.nth(0).unwrap() == "括弧開" {
              if open_kakko_depth >= 1 {
                word_in_kakko.push_str(word)
              }
              open_kakko_depth += 1;
            } else if open_kakko_depth >= 1 {
              word_in_kakko.push_str(word);
            }
          }
          ("」", "記号") => {
            if features.nth(0).unwrap() == "括弧閉" {
              if open_kakko_depth == 0 {
                return Err(anyhow!(
                  "括弧の対応がおかしい at num:{num}, chapter:{chapter:?}"
                ));
              } else if open_kakko_depth == 1 {
                open_kakko_depth = 0;
                // 「とあり」     => before_wordsに追加
                // 「とある」     => before_wordsに追加し、そこで打ち止め
                // 「と、」       => after_wordにし、yomikae_info_lstに追加し初期化
                // 「と読み替える」 => yomikae_info_lstに追加し初期化
                // それ以外         => すべて初期化
                if let Some(node_next1) = node.next() {
                  let word_next1 = &(node_next1.surface)[..(node_next1.length as usize)];
                  let mut features_next1 = node_next1.feature.split(',');
                  let hinshi_next1: &str = features_next1.nth(0).unwrap();
                  match (word_next1, hinshi_next1) {
                    ("と", "助詞") => {
                      node = node_next1;
                      if let Some(node_next2) = node.next() {
                        let word_next2 = &(node_next2.surface)[..(node_next2.length as usize)];
                        let mut features_next2 = node_next2.feature.split(',');
                        let hinshi_next2: &str = features_next2.nth(0).unwrap();
                        match (word_next2, hinshi_next2) {
                          ("あり", "動詞") => {
                            if is_before_words_end {
                              return Err(anyhow!(
                                "文言の並列がおかしい at num:{num}, chapter:{chapter:?}"
                              ));
                            }
                            before_words.push(word_in_kakko);
                            is_before_words_end = false;
                            node = node_next2;
                          }
                          ("ある", "動詞") => {
                            before_words.push(word_in_kakko);
                            is_before_words_end = true;
                            node = node_next2;
                          }
                          ("、", "記号") => {
                            if features_next2.nth(0).unwrap() == "読点" {
                              let yomikae_info = YomikaeInfo {
                                num: num.to_string(),
                                chapter: chapter.clone(),
                                before_words: before_words,
                                after_word: word_in_kakko,
                              };
                              yomikae_info_lst.push(yomikae_info);
                              is_before_words_end = false;
                            }
                            before_words = vec![];
                            node = node_next2;
                          }
                          ("読み替える", "動詞") => {
                            let yomikae_info = YomikaeInfo {
                              num: num.to_string(),
                              chapter: chapter.clone(),
                              before_words: before_words,
                              after_word: word_in_kakko,
                            };
                            yomikae_info_lst.push(yomikae_info);
                            is_before_words_end = false;
                            before_words = vec![];
                            node = node_next2;
                          }
                          _ => {
                            before_words = vec![];
                          }
                        }
                      } else {
                      }
                    }
                    _ => {
                      before_words = vec![];
                    }
                  }
                } else {
                  before_words = vec![];
                }
              } else {
                word_in_kakko.push_str(word)
              }
            }
            word_in_kakko = String::new();
          }
          (_, _) => {
            if open_kakko_depth >= 1 {
              word_in_kakko.push_str(word);
            }
          }
        }
      }
    }
    if let Some(new_node) = node.next() {
      node = new_node
    } else {
      break;
    }
  }

  Ok(yomikae_info_lst)
}

#[tokio::test]
async fn check1() {
  let lawtext = LawText {
      is_child : false,
      contents : "この場合において、第八百五十一条第四号中「被後見人を代表する」とあるのは、「被保佐人を代表し、又は被保佐人がこれをすることに同意する」と読み替えるものとする。".to_string()
    };
  let chapter = Chapter {
    part: None,
    chapter: None,
    section: None,
    subsection: None,
    division: None,
    article: String::from("test"),
    paragraph: None,
    item: None,
    sub_item: None,
    suppl_provision_title: None,
  };
  let yomikae_info_lst = parse_yomikae(
    &lawtext,
    "test",
    &chapter,
    "/usr/lib/x86_64-linux-gnu/mecab/dic/mecab-ipadic-neologd",
  )
  .await
  .unwrap();
  assert_eq!(
    vec![YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter,
      before_words: vec!["被後見人を代表する".to_string()],
      after_word: "被保佐人を代表し、又は被保佐人がこれをすることに同意する".to_string()
    }],
    yomikae_info_lst
  )
}

#[tokio::test]
async fn check2() {
  let lawtext = LawText {
      is_child : false,
      contents : "この場合において、同条中「子ども・子育て支援法（平成二十四年法律第六十五号）第六十九条」とあるのは「平成二十二年度等における子ども手当の支給に関する法律（平成二十二年法律第十九号）第二十条第一項の規定により適用される児童手当法の一部を改正する法律（平成二十四年法律第二十四号）附則第十一条の規定によりなおその効力を有するものとされた同法第一条の規定による改正前の児童手当法（昭和四十六年法律第七十三号）第二十条」と、「子ども・子育て拠出金」とあるのは「子ども手当拠出金」と読み替えるものとする。".to_string()
    };
  let chapter = Chapter {
    part: None,
    chapter: None,
    section: None,
    subsection: None,
    division: None,
    article: String::from("test"),
    paragraph: None,
    item: None,
    sub_item: None,
    suppl_provision_title: None,
  };
  let yomikae_info_lst = parse_yomikae(
    &lawtext,
    "test",
    &chapter,
    "/usr/lib/x86_64-linux-gnu/mecab/dic/mecab-ipadic-neologd",
  )
  .await
  .unwrap();
  assert_eq!(
    vec![YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec!["子ども・子育て支援法（平成二十四年法律第六十五号）第六十九条".to_string()],
      after_word: "平成二十二年度等における子ども手当の支給に関する法律（平成二十二年法律第十九号）第二十条第一項の規定により適用される児童手当法の一部を改正する法律（平成二十四年法律第二十四号）附則第十一条の規定によりなおその効力を有するものとされた同法第一条の規定による改正前の児童手当法（昭和四十六年法律第七十三号）第二十条".to_string()
    },YomikaeInfo{
      num:"test".to_string(),
      chapter: chapter,
      before_words :vec!["子ども・子育て拠出金".to_string()],
      after_word : "子ども手当拠出金".to_string()
    }],
    yomikae_info_lst
  )
}

#[tokio::test]
async fn check3() {
  let lawtext = LawText {
      is_child : false,
      contents : "この場合において、同項中「それぞれ同項各号に定める者」とあり、及び同項第二号中「その者」とあるのは、「都道府県の教育委員会」と読み替えるものとする。".to_string()
    };
  let chapter = Chapter {
    part: None,
    chapter: None,
    section: None,
    subsection: None,
    division: None,
    article: String::from("test"),
    paragraph: None,
    item: None,
    sub_item: None,
    suppl_provision_title: None,
  };
  let yomikae_info_lst = parse_yomikae(
    &lawtext,
    "test",
    &chapter,
    "/usr/lib/x86_64-linux-gnu/mecab/dic/mecab-ipadic-neologd",
  )
  .await
  .unwrap();
  assert_eq!(
    vec![YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter,
      before_words: vec![
        "それぞれ同項各号に定める者".to_string(),
        "その者".to_string()
      ],
      after_word: "都道府県の教育委員会".to_string()
    }],
    yomikae_info_lst
  )
}
