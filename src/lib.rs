use jplaw_text::{LawContents, LawText};
use mecab::Tagger;
use search_article_with_word::Chapter;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Hash, Deserialize)]
pub struct LawInfo {
  pub num: String,
  pub chapter: Chapter,
  pub contents: LawText,
}

#[derive(Debug, Error, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum YomikaeError {
  #[error("Do not analysis table contents at {0:?}")]
  ContentsOfTable(LawInfo),
  #[error("Unmatched parentheses at {0:?}")]
  UnmatchedParen(LawInfo),
  #[error("Unexpected parallel words at {0:?}")]
  UnexpectedParallelWords(LawInfo),
  #[error("Not found yomikae sentence at {0:?}")]
  NotFoundYomikae(LawInfo),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct YomikaeInfo {
  /// 法律番号
  pub num: String,
  /// その読み替え規定がある条項
  pub chapter: Chapter,
  /// 読み替えられる前の単語
  pub before_words: Vec<String>,
  /// 読み替えられた後の単語
  pub after_word: String,
}

/// 読み替え規定文は
/// 「((「〜〜」とあり)*「〜〜」とあるのは「〜〜」(と、|と))+読み替えるものとする。」
/// のような形になっている（読点の有無等の違いは微妙にはある）
pub async fn parse_yomikae(
  law_text: &LawText,
  num: &str,
  chapter: &Chapter,
  mecab_ipadic_path: &str,
) -> Result<Vec<YomikaeInfo>, YomikaeError> {
  let law_info = LawInfo {
    num: num.to_string(),
    chapter: chapter.clone(),
    contents: law_text.clone(),
  };
  let input = &law_text.contents;
  match input {
    LawContents::Text(input) => {
      info!("[INPUT] {num} : {:?}", input);
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
                    // 鉤括弧内の鉤括弧であるので、鉤括弧も登場単語として登録する
                    word_in_kakko.push_str(word);
                  }
                  open_kakko_depth += 1;
                } else {
                  word_in_kakko.push_str(word);
                }
              }
              ("」", "記号") => {
                if features.nth(0).unwrap() == "括弧閉" {
                  if open_kakko_depth == 0 {
                    return Err(YomikaeError::UnmatchedParen(law_info));
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
                                  return Err(YomikaeError::UnexpectedParallelWords(law_info));
                                }
                                before_words.push(word_in_kakko);
                                word_in_kakko = String::new();
                                is_before_words_end = false;
                                node = node_next2;
                              }
                              ("ある", "動詞") => {
                                before_words.push(word_in_kakko);
                                word_in_kakko = String::new();
                                is_before_words_end = true;
                                node = node_next2;
                              }
                              ("、", "記号") => {
                                if features_next2.nth(0).unwrap() == "読点" {
                                  let yomikae_info = YomikaeInfo {
                                    num: num.to_string(),
                                    chapter: chapter.clone(),
                                    before_words,
                                    after_word: word_in_kakko,
                                  };
                                  word_in_kakko = String::new();
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
                                  before_words,
                                  after_word: word_in_kakko,
                                };
                                word_in_kakko = String::new();
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
                    // 鉤括弧内に出てきた閉じ鉤括弧
                    word_in_kakko.push_str(word);
                    open_kakko_depth -= 1;
                  }
                } else {
                  word_in_kakko.push_str(word);
                }
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

    LawContents::Table(_) => Err(YomikaeError::ContentsOfTable(law_info)),
  }
}

#[tokio::test]
async fn check1() {
  let lawtext = LawText {
      is_child : false,
      contents : LawContents::Text("この場合において、第八百五十一条第四号中「被後見人を代表する」とあるのは、「被保佐人を代表し、又は被保佐人がこれをすることに同意する」と読み替えるものとする。".to_string())
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
      contents : LawContents::Text("この場合において、同条中「子ども・子育て支援法（平成二十四年法律第六十五号）第六十九条」とあるのは「平成二十二年度等における子ども手当の支給に関する法律（平成二十二年法律第十九号）第二十条第一項の規定により適用される児童手当法の一部を改正する法律（平成二十四年法律第二十四号）附則第十一条の規定によりなおその効力を有するものとされた同法第一条の規定による改正前の児童手当法（昭和四十六年法律第七十三号）第二十条」と、「子ども・子育て拠出金」とあるのは「子ども手当拠出金」と読み替えるものとする。".to_string())
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
      contents : LawContents::Text("この場合において、同項中「それぞれ同項各号に定める者」とあり、及び同項第二号中「その者」とあるのは、「都道府県の教育委員会」と読み替えるものとする。".to_string())
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

#[tokio::test]
async fn check4() {
  let lawtext = LawText {
      is_child : false,
      contents : LawContents::Text("この場合において、徴収法施行規則第二十七条及び第二十八条中「保険関係が成立した」とあるのは「失業保険法及び労働者災害補償保険法の一部を改正する法律及び労働保険の保険料の徴収等に関する法律の施行に伴う関係法律の整備等に関する法律（昭和四十四年法律第八十五号。以下「整備法」という。）第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた」と、「保険関係成立の日」とあるのは「当該保険給付が行なわれることとなつた日」と、徴収法施行規則第二十八条第一項中「全期間」とあるのは「整備法第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた日以後の期間（事業の終了する日前に失業保険法及び労働者災害補償保険法の一部を改正する法律及び労働保険の保険料の徴収等に関する法律の施行に伴う労働省令の整備等に関する省令（昭和四十七年労働省令第九号。以下「整備省令」という。）第八条の期間が経過するときは、その経過する日の前日までの期間）」と、徴収法施行規則第三十二条中「第二十七条から前条まで」とあるのは「第二十七条から第三十条まで」と、「法第十五条から法第十七条まで」とあるのは「法第十五条及び第十六条」と、「その事業の期間」とあるのは「整備法第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた日以後のその事業の期間（事業の終了する日前に整備省令第八条の期間が経過するときは、その経過する日の前日までの期間）」と読み替えるものとする。".to_string())
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
      before_words: vec![
        "保険関係が成立した".to_string()
      ],
      after_word: "失業保険法及び労働者災害補償保険法の一部を改正する法律及び労働保険の保険料の徴収等に関する法律の施行に伴う関係法律の整備等に関する法律（昭和四十四年法律第八十五号。以下「整備法」という。）第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた".to_string()
    },YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec![
        "保険関係成立の日".to_string()
      ],
      after_word: "当該保険給付が行なわれることとなつた日".to_string()
    },YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec![
        "全期間".to_string()
      ],
      after_word: "整備法第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた日以後の期間（事業の終了する日前に失業保険法及び労働者災害補償保険法の一部を改正する法律及び労働保険の保険料の徴収等に関する法律の施行に伴う労働省令の整備等に関する省令（昭和四十七年労働省令第九号。以下「整備省令」という。）第八条の期間が経過するときは、その経過する日の前日までの期間）".to_string()
    },YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec![
        "第二十七条から前条まで".to_string()
      ],
      after_word: "第二十七条から第三十条まで".to_string()
    },YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec![
        "法第十五条から法第十七条まで".to_string()
      ],
      after_word: "法第十五条及び第十六条".to_string()
    },YomikaeInfo {
      num: "test".to_string(),
      chapter: chapter.clone(),
      before_words: vec![
        "その事業の期間".to_string()
      ],
      after_word: "整備法第十八条第一項若しくは第二項、第十八条の二第一項若しくは第二項又は第十八条の三第一項若しくは第二項の規定による保険給付が行なわれることとなつた日以後のその事業の期間（事業の終了する日前に整備省令第八条の期間が経過するときは、その経過する日の前日までの期間）".to_string()
    }],
    yomikae_info_lst
  )
}
