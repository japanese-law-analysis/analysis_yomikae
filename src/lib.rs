//! 読み替え文を解析し、読み替えられる対象の文言と、読み替え後の文言を取り出すソフトウェアです。
//!
//!
//! # CLIソフトウェアを使う
//!
//! 実際の法令XMLファイルに対して読み替えの文の解析を行って、情報を取得するソフトウェアです。
//!
//! ## インストール
//!
//! ```sh
//! cargo install --git "https://github.com/japanese-law-analysis/analysis_yomikae.git"
//! ```
//!
//! ## 使い方
//!
//! ```sh
//! analysis_yomikae -o output.json -e err.json -w law_xml -i index.json -a article_info.json
//! ```
//!
//! で起動します。
//!
//! オプションの各意味は以下のとおりです。
//!
//! - `-o`：解析で生成した情報を出力するJSONファイル
//! - `-e`：解析に失敗した条文の情報を出力するJSONファイル
//! - `-w`：法令XMLファイルがあるフォルダ
//! - `-i`：法令のインデックス情報が書かれたJSONファイル [listup_law](https://github.com/japanese-law-analysis/listup_law)で生成するもの
//! - `-a`：具体的に調べる条文の情報が書かれたJSONファイル [search_article_with_word](https://github.com/japanese-law-analysis/search_article_with_word)で生成するもの
//!
//!
//! # ライブラリを使う
//! 詳しくはリポジトリを手元にクローンした上で`cargo doc --open`でドキュメントを生成してください。
//!
//! 解析結果が書かれたJSONファイルに書かれる構造体やエラーの定義がされており、デシリアライズが容易にできるようになっています。
//!
//!
//! ---
//!
//! [MIT License](https://github.com/japanese-law-analysis/analysis_yomikae/blob/master/LICENSE)
//! (c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")
//!

use jplaw_text::{LawContents, LawText};
use search_article_with_word::Chapter;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_stream::StreamExt;
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
#[allow(clippy::iter_nth_zero)]
pub async fn parse_yomikae(
  law_text: &LawText,
  num: &str,
  chapter: &Chapter,
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

      let mut chars_stream = tokio_stream::iter(input.chars());

      let mut yomikae_info_lst = Vec::new();

      // 角カッコの開き
      let mut open_kakko_depth: usize = 0;
      // 角括弧の中にある文字
      let mut word_in_kakko = String::new();

      let mut before_words = Vec::new();
      let mut is_before_words_end = false;

      while let Some(c) = chars_stream.next().await {
        match c {
          '「' => {
            if open_kakko_depth >= 1 {
              // 鉤括弧内の鉤括弧であるので、鉤括弧も登場単語として登録する
              word_in_kakko.push(c);
            }
            open_kakko_depth += 1;
          }
          '」' => {
            if open_kakko_depth == 0 {
              return Err(YomikaeError::UnmatchedParen(law_info));
            } else if open_kakko_depth == 1 {
              open_kakko_depth = 0;
              // 「とあり」     => before_wordsに追加
              // 「とある」     => before_wordsに追加し、そこで打ち止め
              // 「と、」       => after_wordにし、yomikae_info_lstに追加し初期化
              // 「と読み替える」 => yomikae_info_lstに追加し初期化
              // 「と「」         => 「と、」と基本同じ
              // それ以外         => すべて初期化
              if let Some('と') = chars_stream.next().await {
                if let Some(c_next2) = chars_stream.next().await {
                  match c_next2 {
                    'あ' => {
                      if let Some(c_next3) = chars_stream.next().await {
                        match c_next3 {
                          'り' => {
                            if is_before_words_end {
                              return Err(YomikaeError::UnexpectedParallelWords(law_info));
                            }
                            before_words.push(word_in_kakko);
                            word_in_kakko = String::new();
                            is_before_words_end = false;
                          }
                          'る' => {
                            before_words.push(word_in_kakko);
                            word_in_kakko = String::new();
                            is_before_words_end = true;
                          }
                          _ => before_words = vec![],
                        }
                      }
                    }
                    '、' => {
                      let yomikae_info = YomikaeInfo {
                        num: num.to_string(),
                        chapter: chapter.clone(),
                        before_words: before_words.clone(),
                        after_word: word_in_kakko,
                      };
                      word_in_kakko = String::new();
                      yomikae_info_lst.push(yomikae_info);
                      is_before_words_end = false;
                      before_words = vec![];
                    }
                    '読' => {
                      if let Some('み') = chars_stream.next().await {
                        if let Some('替') = chars_stream.next().await {
                          if let Some('え') = chars_stream.next().await {
                            if let Some('る') = chars_stream.next().await {
                              let yomikae_info = YomikaeInfo {
                                num: num.to_string(),
                                chapter: chapter.clone(),
                                before_words: before_words.clone(),
                                after_word: word_in_kakko,
                              };
                              word_in_kakko = String::new();
                              yomikae_info_lst.push(yomikae_info);
                              is_before_words_end = false;
                              before_words = vec![];
                            }
                          }
                        }
                      }
                    }
                    '「' => {
                      // 終了処理をしてすぐに開始する
                      let yomikae_info = YomikaeInfo {
                        num: num.to_string(),
                        chapter: chapter.clone(),
                        before_words: before_words.clone(),
                        after_word: word_in_kakko,
                      };
                      word_in_kakko = String::new();
                      yomikae_info_lst.push(yomikae_info);
                      is_before_words_end = false;
                      before_words = vec![];

                      open_kakko_depth += 1;
                    }
                    _ => {
                      before_words = vec![];
                    }
                  }
                } else {
                }
              } else {
                before_words = vec![];
              }
            } else {
              // 鉤括弧内に出てきた閉じ鉤括弧
              word_in_kakko.push(c);
              open_kakko_depth -= 1;
            }
          }
          _ => {
            if open_kakko_depth >= 1 {
              word_in_kakko.push(c);
            }
          }
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
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
async fn check2_2() {
  let lawtext = LawText {
      is_child : false,
      contents : LawContents::Text("この場合において、同条中「子ども・子育て支援法（平成二十四年法律第六十五号）第六十九条」とあるのは「平成二十二年度等における子ども手当の支給に関する法律（平成二十二年法律第十九号）第二十条第一項の規定により適用される児童手当法の一部を改正する法律（平成二十四年法律第二十四号）附則第十一条の規定によりなおその効力を有するものとされた同法第一条の規定による改正前の児童手当法（昭和四十六年法律第七十三号）第二十条」と「子ども・子育て拠出金」とあるのは「子ども手当拠出金」と読み替えるものとする。".to_string())
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
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

#[tokio::test]
async fn check5() {
  let lawtext = LawText {
      is_child : false,
      contents : LawContents::Text("第百十三条の三十八の規定は、調査員養成研修について準用する。この場合において、同条第一項中「法第六十九条の三十三第一項」とあるのは「令第三十七条の七第一項」と、同項第五号中「前条」とあるのは「第百十三条の三十七」と、同条第二項中「令第三十五条の十六第一項第二号イ」とあるのは「令第三十七条の七第四項第三号イ」と、同条第三項中「令第三十五条の十六第一項第二号ロ」とあるのは「令第三十七条の七第四項第三号ロ」と、同条第四項中「令第三十五条の十六第一項第二号ハ」とあるのは「令第三十七条の七第四項第三号ハ」と「実務研修受講試験の合格年月日並びに研修の受講の開始年月日」とあるのは「研修の受講の開始年月日」と読み替えるものとする。".to_string())
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
  let yomikae_info_lst = parse_yomikae(&lawtext, "test", &chapter).await.unwrap();
  assert_eq!(
    vec![
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["法第六十九条の三十三第一項".to_string()],
        after_word: "令第三十七条の七第一項".to_string()
      },
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["前条".to_string()],
        after_word: "第百十三条の三十七".to_string()
      },
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["令第三十五条の十六第一項第二号イ".to_string()],
        after_word: "令第三十七条の七第四項第三号イ".to_string()
      },
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["令第三十五条の十六第一項第二号ロ".to_string()],
        after_word: "令第三十七条の七第四項第三号ロ".to_string()
      },
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["令第三十五条の十六第一項第二号ハ".to_string()],
        after_word: "令第三十七条の七第四項第三号ハ".to_string()
      },
      YomikaeInfo {
        num: "test".to_string(),
        chapter: chapter.clone(),
        before_words: vec!["実務研修受講試験の合格年月日並びに研修の受講の開始年月日".to_string()],
        after_word: "研修の受講の開始年月日".to_string()
      }
    ],
    yomikae_info_lst
  )
}
